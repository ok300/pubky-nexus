use crate::service::utils::{
    create_random_homeservers_and_persist, setup, MockEventProcessorResult,
    MockEventProcessorRunner,
};
use anyhow::Result;
use nexus_watcher::service::TEventProcessorRunner;
use std::time::Duration;

#[tokio_shared_rt::test(shared)]
async fn test_multiple_homeserver_event_processing() -> Result<()> {
    // Initialize the test
    let mut event_processor_list = setup().await?;
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create 4 random homeservers with success result
    // The first one will be the default homeserver (processed via run_default, not run_all)
    for _ in 0..4 {
        let processor_status = MockEventProcessorResult::Success;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            None,
            processor_status,
            None,
            shutdown_rx.clone(),
        )
        .await;
    }

    // Create 1 random homeserver with error result
    let processor_status = MockEventProcessorResult::Error("PubkyClient: timeout from HS".into());
    create_random_homeservers_and_persist(
        &mut event_processor_list,
        None,
        processor_status,
        None,
        shutdown_rx.clone(),
    )
    .await;

    let runner = MockEventProcessorRunner::new(event_processor_list, 5, shutdown_rx);

    // run_all excludes the default homeserver, so we expect 3 successful (4 - 1 default) and 1 error
    let stats = runner.run_all().await.unwrap().0;
    assert_eq!(stats.count_ok(), 3);
    assert_eq!(stats.count_error(), 1);
    assert_eq!(stats.count_panic(), 0);
    assert_eq!(stats.count_timeout(), 0);

    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn test_run_default_processes_only_default_homeserver() -> Result<()> {
    // Initialize the test
    let mut event_processor_list = setup().await?;
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create 3 random homeservers with success result
    // The first one will be the default homeserver
    for _ in 0..3 {
        let processor_status = MockEventProcessorResult::Success;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            None,
            processor_status,
            None,
            shutdown_rx.clone(),
        )
        .await;
    }

    let runner = MockEventProcessorRunner::new(event_processor_list, 3, shutdown_rx);

    // run_default should process only the default homeserver
    let stats = runner.run_default().await.unwrap().0;
    assert_eq!(stats.count_ok(), 1);
    assert_eq!(stats.count_error(), 0);
    assert_eq!(stats.count_panic(), 0);
    assert_eq!(stats.count_timeout(), 0);

    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn test_multi_hs_event_processing_with_homeserver_limit() -> Result<()> {
    // Initialize the test
    let mut event_processor_list = setup().await?;
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create 6 random homeservers (first is default, others are non-default)
    for _ in 0..6 {
        let processor_status = MockEventProcessorResult::Success;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            None,
            processor_status,
            None,
            shutdown_rx.clone(),
        )
        .await;
    }

    assert_eq!(event_processor_list.len(), 6); // Ensure 6 HSs are available
    let hs_limit = 3; // Configure a monitored_homeservers_limit of 3
    let runner = MockEventProcessorRunner::new(event_processor_list, hs_limit, shutdown_rx);

    // run_all excludes the default, so we have 5 non-default HSs, limited to 3
    let stats = runner.run_all().await.unwrap().0;
    assert_eq!(stats.count_ok(), 3); // 3 successful ones, due to the limit
    assert_eq!(stats.count_timeout(), 0);
    assert_eq!(stats.count_error(), 0);
    assert_eq!(stats.count_panic(), 0);

    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn test_multi_hs_event_processing_with_homeserver_limit_one() -> Result<()> {
    // Initialize the test
    let mut event_processor_list = setup().await?;
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create 5 random homeservers
    for _ in 0..5 {
        let processor_status = MockEventProcessorResult::Success;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            None,
            processor_status,
            None,
            shutdown_rx.clone(),
        )
        .await;
    }

    assert_eq!(event_processor_list.len(), 5); // Ensure 5 HSs are available

    // Check that, when the limit is 1, only 1 non-default homeserver is considered by run_all
    // (the default is processed separately via run_default)
    let runner_one = MockEventProcessorRunner::new(event_processor_list, 1, shutdown_rx);
    let hs_list = runner_one.pre_run_all().await.unwrap();
    assert_eq!(hs_list.len(), 1);
    // The hs in the list should NOT be the default homeserver
    assert_ne!(
        hs_list.get(0).unwrap(),
        &runner_one.default_homeserver(),
        "pre_run_all should not include the default homeserver"
    );

    let stats_one = runner_one.run_all().await.unwrap().0;
    assert_eq!(stats_one.count_ok(), 1); // 1 successful, due to the limit
    assert_eq!(stats_one.count_timeout(), 0);
    assert_eq!(stats_one.count_error(), 0);
    assert_eq!(stats_one.count_panic(), 0);

    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn test_multi_hs_event_processing_with_timeout() -> Result<()> {
    const EVENT_PROCESSOR_TIMEOUT: Option<Duration> = Some(Duration::from_secs(1));
    // Initialize the test
    let mut event_processor_list = setup().await?;
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create 4 random homeservers with timeout limit
    // First one (index 0) is default, processed via run_default
    // Others (indices 1, 2, 3) are processed via run_all
    for index in 0..4 {
        let processor_status = MockEventProcessorResult::Success;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            Some(Duration::from_secs(index * 2)),
            processor_status,
            EVENT_PROCESSOR_TIMEOUT,
            shutdown_rx.clone(),
        )
        .await;
    }

    let runner = MockEventProcessorRunner::new(event_processor_list, 4, shutdown_rx);

    // run_all excludes index 0 (default), processes indices 1, 2, 3:
    // - index 1: sleep 2s, timeout 1s -> times out
    // - index 2: sleep 4s, timeout 1s -> times out
    // - index 3: sleep 6s, timeout 1s -> times out
    let stats = runner.run_all().await.unwrap().0;
    assert_eq!(stats.count_ok(), 0); // 0 success (all time out)
    assert_eq!(stats.count_timeout(), 3); // 3 failures due to timeout
    assert_eq!(stats.count_error(), 0);
    assert_eq!(stats.count_panic(), 0);

    Ok(())
}

#[tokio_shared_rt::test(shared)]
async fn test_multi_hs_event_processing_with_panic() -> Result<()> {
    // Initialize the test
    let mut event_processor_list = setup().await?;
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create 4 random homeservers expected to succeed (first is default)
    for _i in 0..4 {
        let processor_status = MockEventProcessorResult::Success;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            None,
            processor_status,
            None,
            shutdown_rx.clone(),
        )
        .await;
    }

    // Create 2 random homeservers expected to panic
    for _i in 0..2 {
        let processor_status = MockEventProcessorResult::Panic;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            None,
            processor_status,
            None,
            shutdown_rx.clone(),
        )
        .await;
    }

    let runner = MockEventProcessorRunner::new(event_processor_list, 6, shutdown_rx);

    // run_all excludes the default homeserver, so we have 3 success + 2 panic = 5 non-default
    let stats = runner.run_all().await.unwrap().0;
    assert_eq!(stats.count_ok(), 3); // 3 expected to succeed (4 - 1 default)
    assert_eq!(stats.count_timeout(), 0);
    assert_eq!(stats.count_error(), 0);
    assert_eq!(stats.count_panic(), 2); // 2 expected to panic

    Ok(())
}
