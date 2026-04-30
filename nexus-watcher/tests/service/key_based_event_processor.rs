use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use nexus_common::db::{exec_single_row, queries};
use nexus_common::models::homeserver::Homeserver;
use nexus_common::models::user::UserDetails;
use nexus_common::types::DynError;
use nexus_watcher::events::retry::{InitialBackoff, RetryScheduler};
use nexus_watcher::events::EventHandler;
use nexus_watcher::service::indexer::{KeyBasedEventProcessor, TEventProcessor};
use pubky::{Event as StreamEvent, EventCursor, EventType, Keypair, PubkyResource, PublicKey};
use pubky_app_specs::PubkyId;

use crate::service::utils::{
    create_mock_handler, new_in_memory_store, setup, MockKeyBasedEventSource,
};

#[tokio_shared_rt::test(shared)]
async fn key_based_processor_skips_unrecognized_events() -> Result<(), DynError> {
    setup().await?;

    let (_hs_keypair, homeserver) = create_homeserver().await?;
    let user_id = create_user_on_homeserver(&homeserver).await?;
    let source = Arc::new(MockKeyBasedEventSource::default().with_events(vec![vec![
        stream_event(1, &user_id, "/pub/other.app/profile.json")?,
        stream_event(2, &user_id, "/pub/pubky.app/profile.json")?,
    ]]));
    let handler = create_mock_handler(Ok(()), None);
    let processor = processor(homeserver, handler.clone(), source.clone());

    processor.run().await?;

    assert_eq!(handler.get_handle_count(), 1);
    assert_eq!(source.calls(), vec![user_id]);

    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn key_based_processor_continues_to_next_user_after_unrecognized_event_then_rejects_wrong_user_event(
) -> Result<(), DynError> {
    setup().await?;

    let (_hs_keypair, homeserver) = create_homeserver().await?;
    let first_user_id = create_user_on_homeserver(&homeserver).await?;
    create_user_on_homeserver(&homeserver).await?;
    let different_user_id = PubkyId::try_from(Keypair::random().public_key().to_z32().as_str())?;
    let different_user_id = different_user_id.to_string();
    let source = Arc::new(MockKeyBasedEventSource::default().with_events(vec![
        vec![stream_event(
            1,
            &first_user_id,
            "/pub/other.app/profile.json",
        )?],
        vec![
            stream_event(2, &different_user_id, "/pub/pubky.app/profile.json")?,
            stream_event(3, &different_user_id, "/pub/pubky.app/posts/after-mismatch")?,
        ],
    ]));
    let handler = create_mock_handler(Ok(()), None);
    let processor = processor(homeserver, handler.clone(), source.clone());

    let result = processor.run().await;

    assert!(result.is_ok());
    assert_eq!(handler.get_handle_count(), 0);
    assert_eq!(source.calls().len(), 2);

    Ok(())
}

async fn create_homeserver() -> Result<(Keypair, Homeserver), DynError> {
    let keypair = Keypair::random();
    let homeserver_id = PubkyId::try_from(keypair.public_key().to_z32().as_str())?;
    let homeserver = Homeserver::new(homeserver_id);
    homeserver.put_to_graph().await?;
    Ok((keypair, homeserver))
}

async fn create_user_on_homeserver(homeserver: &Homeserver) -> Result<String, DynError> {
    let user_id = PubkyId::try_from(Keypair::random().public_key().to_z32().as_str())?;
    let user = UserDetails {
        id: user_id.clone(),
        name: "key-based-processor-test-user".into(),
        bio: None,
        status: None,
        links: None,
        image: None,
        indexed_at: Utc::now().timestamp_millis(),
    };

    exec_single_row(queries::put::create_user(&user)?).await?;
    exec_single_row(queries::put::set_user_homeserver(&user_id, &homeserver.id)).await?;

    Ok(user_id.to_string())
}

fn stream_event(cursor: u64, user_id: &str, path: &str) -> Result<StreamEvent, DynError> {
    let user_pk: PublicKey = user_id.parse()?;

    Ok(StreamEvent {
        event_type: EventType::Delete,
        resource: PubkyResource::new(user_pk, path)?,
        cursor: EventCursor::new(cursor),
    })
}

fn processor(
    homeserver: Homeserver,
    handler: Arc<dyn EventHandler>,
    source: Arc<MockKeyBasedEventSource>,
) -> Arc<KeyBasedEventProcessor> {
    Arc::new(KeyBasedEventProcessor {
        homeserver,
        limit: 100,
        files_path: PathBuf::from("/tmp/nexus-watcher-test"),
        event_handler: handler,
        event_source: source,
        retry_scheduler: Arc::new(RetryScheduler::new(
            new_in_memory_store(),
            InitialBackoff {
                missing_dep_ms: 60_000,
                transient_ms: 10_000,
            },
        )),
        shutdown_rx: tokio::sync::watch::channel(false).1,
    })
}
