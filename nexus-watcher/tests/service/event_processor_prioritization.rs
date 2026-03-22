use crate::event_processor::utils::default_moderation_tests;
use crate::service::utils::HS_IDS;
use crate::service::utils::{create_mock_event_processors, setup, MockEventProcessorRunner};
use anyhow::Result;
use nexus_common::db::exec_single_row;
use nexus_common::db::graph::Query;
use nexus_common::models::homeserver::Homeserver;
use nexus_common::types::DynError;
use nexus_watcher::service::EventProcessorRunner;
use nexus_watcher::service::TEventProcessorRunner;
use pubky_app_specs::PubkyId;
use std::path::PathBuf;
use std::sync::Arc;

/// Helper: creates a User node in the graph and links it to the given homeserver
/// via a HOSTED_BY relationship.
async fn link_test_user_to_hs(user_id: &str, hs_id: &str) -> Result<(), DynError> {
    let query = Query::new(
        "test_link_user_to_hs",
        "MERGE (u:User {id: $user_id})
         WITH u
         MERGE (hs:Homeserver {id: $hs_id})
         MERGE (u)-[:HOSTED_BY]->(hs)",
    )
    .param("user_id", user_id.to_string())
    .param("hs_id", hs_id.to_string());
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

    // Link users to some homeservers, but not HS_IDS[2] (which remains without active users)
    for (i, hs_id) in HS_IDS.iter().enumerate() {
        if i != 2 {
            link_test_user_to_hs(&format!("test_user_priority_{i}"), hs_id).await?;
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

    // Persist the homeservers
    for hs_id in HS_IDS {
        let hs = Homeserver::new(PubkyId::try_from(hs_id).unwrap());
        hs.put_to_graph().await.unwrap();
    }

    // Link users to some homeservers, but not HS_IDS[2] (which remains without active users)
    for (i, hs_id) in HS_IDS.iter().enumerate() {
        if i != 2 {
            link_test_user_to_hs(&format!("test_user_mock_priority_{i}"), hs_id).await?;
        }
    }

    let hs_ids = runner.external_homeservers_by_priority().await?;

    // The default homeserver (HS_IDS[0]) should be excluded from the list
    assert!(
        !hs_ids.contains(&HS_IDS[0].to_string()),
        "Default homeserver should be excluded from homeservers_by_priority"
    );

    // Homeservers with no active users should be excluded
    assert!(
        !hs_ids.contains(&HS_IDS[2].to_string()),
        "Homeserver with no active users should be excluded"
    );

    Ok(())
}
