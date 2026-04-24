use chrono::Utc;
use tokio::sync::watch::Receiver;

/// Returns the current Unix timestamp in milliseconds as `u64`.
pub fn current_time_millis() -> u64 {
    u64::try_from(Utc::now().timestamp_millis())
        .expect("system clock must be at or after Unix epoch")
}

/// Creates a watch channel that can be used for shutdown signalling.
///
/// On Ctrl-C, it sends a signal that can be picked up by the receiver returned.
pub fn create_shutdown_rx() -> Receiver<bool> {
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = shutdown_tx.send(true);
    });
    shutdown_rx
}
