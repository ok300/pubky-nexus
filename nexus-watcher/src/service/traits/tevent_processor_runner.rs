use std::{sync::Arc, time::Instant};

use nexus_common::types::DynError;
use tokio::sync::watch::Receiver;
use tracing::{debug, error, info, warn};

use crate::service::{
    stats::{ProcessedStats, ProcessorRunStatus, RunAllProcessorsStats},
    traits::{tevent_processor::RunError, TEventProcessor},
};

/// Asynchronous wrapper that helps build and run event processors in the Watcher service.
///
/// # Implementation Notes
/// - The `build` method should create and return a fully configured event processor
///   ready for immediate use
/// - Implementors should ensure that created processors are properly isolated and
///   don't share mutable state unless explicitly intended
///
/// # Parallel Processing Model
/// The runner supports parallel processing of homeservers. On each interval tick,
/// two methods are designed to run concurrently:
/// - [`TEventProcessorRunner::run_default`] processes only the default homeserver from the configuration
/// - [`TEventProcessorRunner::run_all`] processes all other homeservers returned by [`TEventProcessorRunner::homeservers_by_priority`],
///   which excludes the default homeserver
///
/// This parallel execution ensures the default homeserver is always processed promptly,
/// regardless of the load from other homeservers.
#[async_trait::async_trait]
pub trait TEventProcessorRunner {
    /// Returns the shutdown signal receiver
    fn shutdown_rx(&self) -> Receiver<bool>;

    /// Returns the default homeserver ID for this runner.
    /// This homeserver is processed separately via [`TEventProcessorRunner::run_default`] and is excluded
    /// from [`TEventProcessorRunner::homeservers_by_priority`].
    fn default_homeserver(&self) -> &str;

    fn monitored_homeservers_limit(&self) -> usize;

    /// Returns the homeserver IDs relevant for [`run_all`], ordered by their priority.
    ///
    /// Contains all homeserver IDs from the graph, **excluding** the default homeserver.
    /// The default homeserver is processed separately via [`run_default`].
    async fn homeservers_by_priority(&self) -> Result<Vec<String>, DynError>;

    /// Creates and returns a new event processor instance for the specified homeserver.
    ///
    /// # Parameters
    /// * `homeserver_id` - The homeserver PubkyId. Represents the homeserver this event processor will
    /// fetch and process events from.
    ///
    /// # Returns
    /// A reference to the event processor instance, ready to be executed with its `run` method.
    ///
    /// # Errors
    /// Throws a [`DynError`] if the event processor couldn't be built
    async fn build(&self, homeserver_id: String) -> Result<Arc<dyn TEventProcessor>, DynError>;

    /// Decides the amount and order of homeservers from which events will be fetched and processed in `run_all`.
    ///
    /// # Returns
    /// Considers the values of [TEventProcessorRunner::homeservers_by_priority].
    /// Depending on [TEventProcessorRunner::monitored_homeservers_limit], only a subset of this list may be returned.
    async fn pre_run_all(&self) -> Result<Vec<String>, DynError> {
        let hs_ids = self.homeservers_by_priority().await?;
        let max_index = std::cmp::min(self.monitored_homeservers_limit(), hs_ids.len());
        Ok(hs_ids[..max_index].to_vec())
    }

    /// Post-processing of the run results
    async fn post_run_all(&self, stats: RunAllProcessorsStats) -> ProcessedStats {
        for individual_run_stat in &stats.stats {
            let hs_id = &individual_run_stat.hs_id;
            let duration = individual_run_stat.duration;
            let status = &individual_run_stat.status;
            debug!("Event processor run for HS {hs_id}: duration {duration:?}, status {status:?}");
        }

        let count_ok = stats.count_ok();
        let count_error = stats.count_error();
        let count_panic = stats.count_panic();
        let count_timeout = stats.count_timeout();
        let count_failed_to_build = stats.count_failed_to_build();
        let had_issues = count_error + count_panic + count_timeout + count_failed_to_build > 0;

        if had_issues {
            warn!(
                "Run result: {count_ok} ok, {count_failed_to_build} failed to build, {count_error} error, {count_panic} panic, {count_timeout} timeout"
            );
        } else {
            debug!("Run result: {count_ok} ok");
        }

        ProcessedStats(stats)
    }

    /// Runs event processors for all homeservers (excluding the default) relevant for this run, with timeout protection.
    ///
    /// The default homeserver is processed separately via [`run_default`].
    ///
    /// # Returns
    /// Statistics about the event processor run results, summarized as [`RunAllProcessorsStats`]
    async fn run_all(&self) -> Result<ProcessedStats, DynError> {
        let hs_ids = self.pre_run_all().await?;

        let mut run_stats = RunAllProcessorsStats::default();

        for hs_id in hs_ids {
            if *self.shutdown_rx().borrow() {
                info!("Shutdown detected in homeserver {hs_id}, exiting run_all loop");
                break; // Exit loop
            }

            let t0 = Instant::now();
            let status = match self.build(hs_id.clone()).await {
                Ok(event_processor) => match event_processor.run().await {
                    Ok(_) => ProcessorRunStatus::Ok,
                    Err(RunError::Internal(_)) => ProcessorRunStatus::Error,
                    Err(RunError::Panicked) => ProcessorRunStatus::Panic,
                    Err(RunError::TimedOut) => ProcessorRunStatus::Timeout,
                },
                Err(e) => {
                    error!("Failed to build event processor for homeserver: {hs_id}: {e}");
                    ProcessorRunStatus::FailedToBuild
                }
            };
            let duration = Instant::now().duration_since(t0);

            run_stats.add_run_result(hs_id, duration, status);
        }

        let processed_stats = self.post_run_all(run_stats).await;
        Ok(processed_stats)
    }

    /// Runs the event processor for only the default homeserver.
    ///
    /// This method is intended to be run in parallel with [`run_all`], which processes
    /// all other homeservers.
    ///
    /// # Returns
    /// Statistics about the event processor run result for the default homeserver.
    async fn run_default(&self) -> Result<ProcessedStats, DynError> {
        let default_hs_id = self.default_homeserver().to_string();

        let mut run_stats = RunAllProcessorsStats::default();

        if *self.shutdown_rx().borrow() {
            info!("Shutdown detected, skipping default homeserver processing");
            let processed_stats = self.post_run_all(run_stats).await;
            return Ok(processed_stats);
        }

        let t0 = Instant::now();
        let status = match self.build(default_hs_id.clone()).await {
            Ok(event_processor) => match event_processor.run().await {
                Ok(_) => ProcessorRunStatus::Ok,
                Err(RunError::Internal(_)) => ProcessorRunStatus::Error,
                Err(RunError::Panicked) => ProcessorRunStatus::Panic,
                Err(RunError::TimedOut) => ProcessorRunStatus::Timeout,
            },
            Err(e) => {
                error!(
                    "Failed to build event processor for default homeserver: {default_hs_id}: {e}"
                );
                ProcessorRunStatus::FailedToBuild
            }
        };
        let duration = Instant::now().duration_since(t0);

        run_stats.add_run_result(default_hs_id, duration, status);

        let processed_stats = self.post_run_all(run_stats).await;
        Ok(processed_stats)
    }
}
