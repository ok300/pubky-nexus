use crate::event_processor::utils::watcher::WatcherTest;

use anyhow::Result;
use nexus_common::types::DynError;

#[tokio_shared_rt::test(shared)]
async fn test_parallel_event_processing() -> Result<(), DynError> {
    let mut test = WatcherTest::setup().await?;

    test.ensure_event_processing_complete().await?;

    // TODO If we monitor 3 homeservers, ensure we process events from all 3

    Ok(())
}

// TODO Check that run is interrupted on shutdown signal
// TODO Check that run is not affected if one event processor throws error, or timeouts, or panics
// TODO Test how an individual event processor reacts to shutdown signal (if unresponsive, will entire run hang?)
// TODO Test what happens if a run (triggered by tick) runs longer than the configured repeat interval
// TODO Test various timeout scenarios: connection timeout during processor run, entire processor run taking longer than MAX_DURATION(?)
// TODO Test that Homeserver::get_all_from_graph() throws error if empty list

// TODO Log each run duration (individual, total)
