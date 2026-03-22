use crate::event_processor::utils::default_moderation_tests;
use crate::service::utils::HS_IDS;
use crate::service::utils::{create_mock_event_processors, setup, MockEventProcessorRunner};
use anyhow::Result;
use nexus_common::db::exec_single_row;
use nexus_common::db::graph::Query;
use nexus_common::db::queries;
use nexus_common::models::homeserver::Homeserver;
use nexus_common::types::DynError;
use nexus_watcher::service::EventProcessorRunner;
use nexus_watcher::service::TEventProcessorRunner;
use pubky_app_specs::PubkyId;
use std::path::PathBuf;
use std::sync::Arc;

/// Helper: creates a minimal User node in the graph.
async fn create_test_user(user_id: &str) -> Result<(), DynError> {
    let query = Query::new(
        "create_test_user",
        "MERGE (u:User {id: $id})
         SET u.name = 'test', u.indexed_at = 0
         RETURN u;",
    )
    .param("id", user_id);
    exec_single_row(query).await?;
    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn test_event_processor_runner_default_homeserver_excluded() -> Result<(), DynError> {
    // Initialize the test
    setup().await?;

    let runner = EventProcessorRunner {
        default_homeserver: PubkyId::try_from(HS_IDS[3]).unwrap(),
        shutdown_rx: tokio::sync::watch::channel(false).1,
        limit: 1000,
        monitored_homeservers_limit: HS_IDS.len(),
        files_path: PathBuf::from("/tmp/nexus-watcher-test"),
        tracer_name: String::from("unit-test-hs-list-test"),
        moderation: Arc::new(default_moderation_tests()),
    };

    // Persist the homeservers
    for hs_id in HS_IDS {
        let hs = Homeserver::new(PubkyId::try_from(hs_id).unwrap());
        hs.put_to_graph().await.unwrap();
    }

    // Create users and link them to some homeservers, but not HS_IDS[2] (which remains without active users)
    for (i, hs_id) in HS_IDS.iter().enumerate() {
        if i != 2 {
            let user_id = format!("test_user_priority_{i}");
            create_test_user(&user_id).await?;
            exec_single_row(queries::put::set_user_homeserver(&user_id, hs_id)).await?;
        }
    }

    let hs_ids = runner.external_homeservers_by_priority().await?;

    // The default homeserver should be excluded from the list
    assert!(
        !hs_ids.contains(&HS_IDS[3].to_string()),
        "Default homeserver should be excluded from homeservers_by_priority"
    );

    // Homeservers with no active users should be excluded
    assert!(
        !hs_ids.contains(&HS_IDS[2].to_string()),
        "Homeserver with no active users should be excluded"
    );

    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn test_mock_event_processor_runner_default_homeserver_excluded() -> Result<(), DynError> {
    // Initialize the test
    setup().await?;

    let event_processors = create_mock_event_processors(None, tokio::sync::watch::channel(false).1)
        .into_iter()
        .map(Arc::new)
        .collect();

    let runner = MockEventProcessorRunner {
        event_processors,
        monitored_homeservers_limit: 100,
        shutdown_rx: tokio::sync::watch::channel(false).1,
    };

    let hs_ids = runner.external_homeservers_by_priority().await?;

    // The default homeserver (HS_IDS[0]) should be excluded from the list
    assert!(
        !hs_ids.contains(&HS_IDS[0].to_string()),
        "Default homeserver should be excluded from external_homeservers_by_priority"
    );

    Ok(())
}
