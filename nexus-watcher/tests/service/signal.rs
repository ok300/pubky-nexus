use crate::service::utils::{
    create_random_homeservers_and_persist, setup, MockEventProcessorResult,
    MockEventProcessorRunner,
};
use anyhow::Result;
use nexus_watcher::service::TEventProcessorRunner;
use std::time::Duration;
use tokio::time::sleep;

#[tokio_shared_rt::test(shared)]
async fn test_shutdown_signal() -> Result<()> {
    // Initialize the test
    let mut event_processor_list = setup().await?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Create 4 random homeservers with different execution durations
    // Index 0: default (0s) - processed via run_default, not run_all
    // Index 1: 2s
    // Index 2: 4s
    // Index 3: 6s
    for index in 0..4 {
        let processor_status = MockEventProcessorResult::Success;
        create_random_homeservers_and_persist(
            &mut event_processor_list,
            Some(Duration::from_secs(index * 2)),
            processor_status,
            None,
            shutdown_rx.clone(),
        )
        .await;
    }

    let runner = MockEventProcessorRunner::new(event_processor_list, 4, shutdown_rx);

    // Schedule Ctrl-C simulation after 3s
    tokio::spawn({
        let shutdown_tx = shutdown_tx.clone();
        async move {
            sleep(Duration::from_secs(3)).await;
            let _ = shutdown_tx.send(true);
        }
    });

    let stats = runner.run_all().await.unwrap().0;

    // run_all excludes the default (index 0), so it processes indices 1, 2, 3:
    // - index 1 (2s): completes before shutdown (3s)
    // - index 2 (4s): starts, but shutdown signal sent while running, exits gracefully
    // - index 3 (6s): may not even start if shutdown detected in run_all loop
    // Expected: 2 successful (index 1 + index 2 which exits early on shutdown)
    assert_eq!(stats.count_ok(), 2); // 2 processors run without errors
    assert_eq!(stats.count_error(), 0); // no processors fail
    assert_eq!(stats.count_panic(), 0);
    assert_eq!(stats.count_timeout(), 0);

    Ok(())
}
