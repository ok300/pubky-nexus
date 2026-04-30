use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use nexus_common::models::event::EventProcessorError;
use nexus_watcher::service::{KeyBasedEventSource, KeyBasedRawEvent};
use pubky::{EventCursor, PublicKey};

#[derive(Default)]
pub struct MockKeyBasedEventSource {
    /// Events returned for a specific user ID when the mock is keyed by user.
    events_by_user: Mutex<HashMap<String, Vec<KeyBasedRawEvent>>>,
    /// Events returned by fetch order, useful when user resolution order is not important.
    events_by_call: Mutex<VecDeque<Vec<KeyBasedRawEvent>>>,
    /// User IDs requested from the mock, in the order `fetch_events` was called.
    calls: Mutex<Vec<String>>,
}

impl MockKeyBasedEventSource {
    pub fn with_user_events(self, user_id: &str, events: Vec<KeyBasedRawEvent>) -> Self {
        self.events_by_user
            .lock()
            .unwrap()
            .insert(user_id.to_string(), events);
        self
    }

    pub fn with_call_events(self, events: Vec<Vec<KeyBasedRawEvent>>) -> Self {
        *self.events_by_call.lock().unwrap() = events.into();
        self
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl KeyBasedEventSource for MockKeyBasedEventSource {
    async fn fetch_events(
        &self,
        _hs_pk: &PublicKey,
        user_pk: &PublicKey,
        _cursor: EventCursor,
        _limit: u16,
    ) -> Result<Vec<KeyBasedRawEvent>, EventProcessorError> {
        let user_id = user_pk.z32();
        self.calls.lock().unwrap().push(user_id.clone());

        if let Some(events) = self.events_by_call.lock().unwrap().pop_front() {
            return Ok(events);
        }

        Ok(self
            .events_by_user
            .lock()
            .unwrap()
            .get(&user_id)
            .cloned()
            .unwrap_or_default())
    }
}
