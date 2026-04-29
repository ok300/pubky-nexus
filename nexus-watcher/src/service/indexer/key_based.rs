use std::path::PathBuf;
use std::sync::Arc;

use futures::StreamExt;
use nexus_common::db::{PubkyConnector, RedisOps};
use nexus_common::models::event::{Event, EventProcessorError, UserIdMismatch};
use nexus_common::models::homeserver::Homeserver;
use nexus_common::models::user::{user_hs_cursor_key, UserDetails};
use pubky::{Event as StreamEvent, EventCursor, PublicKey};
use pubky_app_specs::PubkyId;
use tokio::sync::watch::Receiver;
use tracing::{debug, error, info};

use super::TEventProcessor;
use crate::events::retry::RetryScheduler;
use crate::events::EventHandler;
use crate::service::user_hs_resolver;

/// Event processor for non-default HSs, where the user-specific `/events-stream` endpoint is used
pub struct KeyBasedEventProcessor {
    /// The HS endpoint this processor fetches events from
    pub homeserver: Homeserver,

    /// Max events the homeserver will send before closing the stream.
    /// Bounds execution time per user, preventing timeout and starvation.
    pub limit: u16,
    pub files_path: PathBuf,
    pub event_handler: Arc<dyn EventHandler>,
    /// Scheduler used to enqueue failed events onto the retry queue
    pub retry_scheduler: Arc<RetryScheduler>,
    pub shutdown_rx: Receiver<bool>,
}

struct RawStreamEvents {
    events: Vec<StreamEvent>,
    terminal_error: Option<EventProcessorError>,
}

struct ValidatedStreamEvent {
    cursor_id: u64,
    event: Event,
}

enum ValidatedStreamItem {
    Event(ValidatedStreamEvent),
    Skipped { cursor_id: u64 },
}

struct ValidatedStreamEvents {
    items: Vec<ValidatedStreamItem>,
    terminal_error: Option<EventProcessorError>,
}

#[async_trait::async_trait]
impl TEventProcessor for KeyBasedEventProcessor {
    fn files_path(&self) -> &PathBuf {
        &self.files_path
    }

    fn event_handler(&self) -> &Arc<dyn EventHandler> {
        &self.event_handler
    }

    fn instance_name(&self) -> String {
        format!("KeyBasedEventProcessor with HS ID: {}", self.homeserver.id)
    }

    fn retry_scheduler(&self) -> Option<&Arc<RetryScheduler>> {
        Some(&self.retry_scheduler)
    }

    async fn run_internal(self: Arc<Self>) -> Result<(), EventProcessorError> {
        let hs_id = self.homeserver.id.to_string();

        let hs_pk: PublicKey = hs_id.parse().map_err(|_| {
            EventProcessorError::client_error(format!("Invalid homeserver public key: {hs_id}"))
        })?;

        let users = self
            .resolve_users_with_cursors(&hs_id)
            .await
            .inspect_err(|e| error!("Failed to resolve users for HS {hs_id}: {e:?}"))?;

        if users.is_empty() {
            debug!("No users on HS {hs_id}, skipping");
            return Ok(());
        }

        info!("Found {} users on HS {hs_id}", users.len());

        // TODO: Process users concurrently (bounded semaphore) to reduce per-HS latency
        //       when many users share a non-default homeserver.
        for (user_pk, cursor) in &users {
            if *self.shutdown_rx.borrow() {
                debug!("Shutdown detected; stopping user iteration for HS {hs_id}");
                break;
            }

            if let Err(err) = self.process_user(&hs_pk, &hs_id, user_pk, *cursor).await {
                let user_id = user_pk.z32();
                if err.is_infrastructure() {
                    error!(
                        hs_id = %hs_id,
                        user = %user_id,
                        action = "abort_hs",
                        error = ?err,
                        "Infrastructure error while processing user; aborting homeserver run",
                    );
                    return Err(err);
                }

                error!(
                    hs_id = %hs_id,
                    user = %user_id,
                    action = "skip_user",
                    error = ?err,
                    "Non-infrastructure user error; continuing with next user",
                );
            }
        }

        Ok(())
    }
}

impl KeyBasedEventProcessor {
    /// Resolves monitored users on this homeserver and reads their cursors from Redis.
    #[tracing::instrument(name = "dx.users.resolve", skip_all, fields(homeserver = %hs_id))]
    async fn resolve_users_with_cursors(
        &self,
        hs_id: &str,
    ) -> Result<Vec<(PublicKey, EventCursor)>, EventProcessorError> {
        let user_ids = user_hs_resolver::get_user_ids_by_homeserver(hs_id).await?;
        debug!("Resolved {} user(s) on HS {hs_id}", user_ids.len());

        let mut users = Vec::with_capacity(user_ids.len());
        for user_id in &user_ids {
            let Ok(user_pk) = user_id.parse::<PublicKey>() else {
                error!("Invalid user public key '{user_id}' on HS {hs_id}, skipping");
                continue;
            };
            let cursor = Self::read_user_cursor(user_id, hs_id).await?;
            users.push((user_pk, cursor));
        }

        Ok(users)
    }

    /// Subscribes to the event stream for a single user and processes incoming events.
    ///
    /// Each user gets their own `limit` budget, ensuring fair progress regardless
    /// of how many events other users have produced.
    #[tracing::instrument(name = "dx.user_events.process", skip_all, fields(
        homeserver = %hs_id,
        user = %user_pk.z32(),
    ))]
    async fn process_user(
        &self,
        hs_pk: &PublicKey,
        hs_id: &str,
        user_pk: &PublicKey,
        cursor: EventCursor,
    ) -> Result<(), EventProcessorError> {
        let user_id = user_pk.z32();
        let mut latest_cursor: Option<u64> = None;

        let result: Result<(), EventProcessorError> = async {
            let stream_events = self
                .get_and_validate_stream_events(hs_pk, hs_id, user_pk, cursor)
                .await?;

            for stream_item in stream_events.items {
                match stream_item {
                    ValidatedStreamItem::Event(stream_event) => {
                        self.handle_event(&stream_event.event).await?;

                        // Always move forward after a success so one bad
                        // event can't block the stream. If handle_event fails with
                        // an infrastructure error, that event will be retried next run.
                        latest_cursor = Some(stream_event.cursor_id);
                    }
                    ValidatedStreamItem::Skipped { cursor_id } => {
                        // Always move forward after a skip so one bad or ignored
                        // event can't block the stream.
                        latest_cursor = Some(cursor_id);
                    }
                }
            }

            if let Some(err) = stream_events.terminal_error {
                return Err(err);
            }

            Ok(())
        }
        .await;

        if let Some(cursor_val) = latest_cursor {
            if let Err(write_err) = Self::write_user_cursor(&user_id, hs_id, cursor_val).await {
                // TODO: Queue failed cursor writes in the retry manager so they
                //       can be recovered without re-processing events.
                error!(
                    hs_id = %hs_id,
                    user = %user_id,
                    cursor = cursor_val,
                    cursor_write_error = ?write_err,
                    "Best-effort cursor persist failed; events may be re-processed on next run",
                );
            }
        }

        result
    }

    /// Gets raw stream events from the Homeserver endpoint, for the given user and cursor.
    ///
    /// These should be validated before being processed or ingested.
    async fn get_stream_events_raw(
        &self,
        hs_pk: &PublicKey,
        hs_id: &str,
        user_pk: &PublicKey,
        cursor: EventCursor,
    ) -> Result<RawStreamEvents, EventProcessorError> {
        let pubky = PubkyConnector::get()?;
        let mut stream = pubky
            .event_stream_for(hs_pk)
            .add_users(vec![(user_pk, Some(cursor))])?
            .limit(self.limit)
            .path("/pub/")
            .subscribe()
            .await
            .inspect_err(|e| error!("Failed to subscribe to event stream: {e:?}"))?;

        let user_id = user_pk.z32();
        let mut events = Vec::new();
        let mut terminal_error = None;

        while let Some(result) = stream.next().await {
            if *self.shutdown_rx.borrow() {
                debug!(hs_id = %hs_id, user = %user_id, "Shutdown detected; exiting event loop");
                break;
            }

            match result {
                Ok(stream_event) => events.push(stream_event),
                Err(err) => {
                    terminal_error = Some(err.into());
                    break;
                }
            }
        }

        Ok(RawStreamEvents {
            events,
            terminal_error,
        })
    }

    /// Wrapper around [Self::get_stream_events_raw] which includes event validation.
    ///
    /// Callers can directly process the resulting events.
    async fn get_and_validate_stream_events(
        &self,
        hs_pk: &PublicKey,
        hs_id: &str,
        user_pk: &PublicKey,
        cursor: EventCursor,
    ) -> Result<ValidatedStreamEvents, EventProcessorError> {
        let raw_events = self
            .get_stream_events_raw(hs_pk, hs_id, user_pk, cursor)
            .await?;
        let user_id = user_pk.z32();
        let expected_user_id = PubkyId::from(user_pk.clone());
        let mut items = Vec::with_capacity(raw_events.events.len());

        for stream_event in raw_events.events {
            let cursor_id = stream_event.cursor.id();

            match Event::from_stream_event(&stream_event, self.files_path.clone()) {
                Ok(Some(event)) => {
                    // Validate event user before handling, since we received it from a 3rd party HS
                    if let Err(err) =
                        Self::validate_user_id(hs_id, &event, expected_user_id.clone())
                    {
                        return Ok(ValidatedStreamEvents {
                            items,
                            terminal_error: Some(err),
                        });
                    }

                    items.push(ValidatedStreamItem::Event(ValidatedStreamEvent {
                        cursor_id,
                        event,
                    }));
                }
                Ok(None) => {
                    items.push(ValidatedStreamItem::Skipped { cursor_id });
                }
                Err(e) => {
                    error!(
                        hs_id = %hs_id,
                        user = %user_id,
                        cursor = cursor_id,
                        "Skipping unparseable stream event: {e}",
                    );
                    items.push(ValidatedStreamItem::Skipped { cursor_id });
                }
            }
        }

        Ok(ValidatedStreamEvents {
            items,
            terminal_error: raw_events.terminal_error,
        })
    }

    fn validate_user_id(
        hs_id: &str,
        event: &Event,
        expected_user_id: PubkyId,
    ) -> Result<(), EventProcessorError> {
        let event_user_id = event.parsed_uri.user_id();
        if event_user_id != &expected_user_id {
            return Err(EventProcessorError::UserIdMismatch(UserIdMismatch {
                hs_id: hs_id.into(),
                expected_user_id: expected_user_id.as_str().into(),
                event_user_id: event_user_id.as_str().into(),
            }));
        }

        Ok(())
    }

    /// Reads the per-user event cursor from the `USER_HS_CURSOR` sorted set in Redis.
    ///
    /// Returns `EventCursor(0)` when the user has no cursor entry (newly ingested).
    /// Propagates Redis errors instead of silently rewinding to 0.
    ///
    /// The cursor is stored as the score (f64) of the homeserver member.
    /// f64 is exact for integer values up to 2^53 (~9 quadrillion), which is
    /// practically unreachable for monotonically incrementing event IDs.
    async fn read_user_cursor(
        user_id: &str,
        hs_id: &str,
    ) -> Result<EventCursor, EventProcessorError> {
        let key = user_hs_cursor_key(user_id);
        let score = UserDetails::check_sorted_set_member(None, &key, &[hs_id]).await?;
        Ok(EventCursor::new(score.unwrap_or(0) as u64))
    }

    /// Persists the per-user event cursor back to the `USER_HS_CURSOR` sorted set.
    async fn write_user_cursor(
        user_id: &str,
        hs_id: &str,
        cursor: u64,
    ) -> Result<(), EventProcessorError> {
        let key = user_hs_cursor_key(user_id);
        UserDetails::put_index_sorted_set(&key, &[(cursor as f64, hs_id)], None, None).await?;
        Ok(())
    }
}
