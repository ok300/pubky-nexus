use std::collections::HashMap;
use std::sync::Mutex;

use tracing::debug;

use crate::service::stats::{ProcessorRunStatus, RunAllProcessorsStats};

/// Maximum number of ticks a homeserver can be skipped before it is retried.
/// With a default tick interval of 5s, this caps backoff at ~160 seconds.
const DEFAULT_MAX_SKIP: u32 = 32;

/// Entry tracking consecutive failures and remaining skips for one homeserver.
struct BackoffEntry {
    consecutive_failures: u32,
    skip_remaining: u32,
}

/// In-memory, tick-count-based exponential backoff tracker for homeservers.
///
/// After each processing cycle, call [`HsBackoff::update_from_stats`] to record
/// successes and failures. Before the next cycle, call
/// [`HsBackoff::filter_backed_off`] to remove homeservers that should be skipped.
///
/// Backoff schedule (ticks to skip):
///   1st failure → 1, 2nd → 2, 3rd → 4, …, capped at `max_skip`.
/// A single success resets the counter for that homeserver.
pub struct HsBackoff {
    state: Mutex<HashMap<String, BackoffEntry>>,
    max_skip: u32,
}

impl Default for HsBackoff {
    fn default() -> Self {
        Self::new()
    }
}

impl HsBackoff {
    /// Creates a new backoff tracker with the default maximum skip count.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            max_skip: DEFAULT_MAX_SKIP,
        }
    }

    /// Creates a new backoff tracker with a custom maximum skip count.
    #[cfg(test)]
    pub fn with_max_skip(max_skip: u32) -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            max_skip,
        }
    }

    /// Records a successful run, clearing any accumulated backoff.
    fn record_success(&self, hs_id: &str) {
        self.state.lock().unwrap().remove(hs_id);
    }

    /// Records a failed run, increasing the exponential backoff.
    fn record_failure(&self, hs_id: &str) {
        let mut state = self.state.lock().unwrap();
        let entry = state
            .entry(hs_id.to_string())
            .or_insert(BackoffEntry {
                consecutive_failures: 0,
                skip_remaining: 0,
            });
        entry.consecutive_failures += 1;
        entry.skip_remaining = std::cmp::min(
            2u32.saturating_pow(entry.consecutive_failures - 1),
            self.max_skip,
        );
        debug!(
            "HS {hs_id}: failure #{}, will skip next {} tick(s)",
            entry.consecutive_failures, entry.skip_remaining
        );
    }

    /// Updates backoff state from the results of a processing cycle.
    pub fn update_from_stats(&self, stats: &RunAllProcessorsStats) {
        for stat in &stats.stats {
            match stat.status {
                ProcessorRunStatus::Ok => self.record_success(&stat.hs_id),
                _ => self.record_failure(&stat.hs_id),
            }
        }
    }

    /// Returns only the homeservers that are **not** currently backed off.
    ///
    /// For each backed-off homeserver, the remaining skip count is decremented.
    /// This method should be called once per processing cycle.
    pub fn filter_backed_off(&self, hs_ids: Vec<String>) -> Vec<String> {
        let mut state = self.state.lock().unwrap();
        hs_ids
            .into_iter()
            .filter(|id| {
                if let Some(entry) = state.get_mut(id) {
                    if entry.skip_remaining > 0 {
                        debug!(
                            "HS {id}: backing off, {} skip(s) remaining",
                            entry.skip_remaining
                        );
                        entry.skip_remaining -= 1;
                        return false;
                    }
                }
                true
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::stats::ProcessorRunStats;
    use std::time::Duration;

    fn make_stat(hs_id: &str, status: ProcessorRunStatus) -> ProcessorRunStats {
        ProcessorRunStats {
            hs_id: hs_id.to_string(),
            duration: Duration::ZERO,
            status,
        }
    }

    #[test]
    fn test_success_clears_backoff() {
        let b = HsBackoff::with_max_skip(8);

        b.record_failure("hs1");
        assert_eq!(
            b.filter_backed_off(vec!["hs1".into()]).len(),
            0,
            "should be backed off after 1 failure"
        );

        b.record_success("hs1");
        assert_eq!(
            b.filter_backed_off(vec!["hs1".into()]).len(),
            1,
            "should be available after success"
        );
    }

    #[test]
    fn test_exponential_skip_counts() {
        let b = HsBackoff::with_max_skip(64);

        // 1st failure → skip 1
        b.record_failure("hs1");
        let state = b.state.lock().unwrap();
        assert_eq!(state["hs1"].skip_remaining, 1);
        drop(state);

        // 2nd failure → skip 2
        b.record_failure("hs1");
        let state = b.state.lock().unwrap();
        assert_eq!(state["hs1"].skip_remaining, 2);
        drop(state);

        // 3rd failure → skip 4
        b.record_failure("hs1");
        let state = b.state.lock().unwrap();
        assert_eq!(state["hs1"].skip_remaining, 4);
        drop(state);
    }

    #[test]
    fn test_max_skip_cap() {
        let b = HsBackoff::with_max_skip(4);

        for _ in 0..10 {
            b.record_failure("hs1");
        }
        let state = b.state.lock().unwrap();
        assert_eq!(state["hs1"].skip_remaining, 4, "should be capped at max");
    }

    #[test]
    fn test_filter_decrements_skip() {
        let b = HsBackoff::with_max_skip(64);

        // 2 consecutive failures → skip_remaining = 2
        b.record_failure("hs1");
        b.record_failure("hs1");

        // Tick 1: skip (remaining goes 2 → 1)
        assert!(b.filter_backed_off(vec!["hs1".into()]).is_empty());

        // Tick 2: skip (remaining goes 1 → 0)
        assert!(b.filter_backed_off(vec!["hs1".into()]).is_empty());

        // Tick 3: included (remaining is 0)
        assert_eq!(b.filter_backed_off(vec!["hs1".into()]).len(), 1);
    }

    #[test]
    fn test_update_from_stats_mixed() {
        let b = HsBackoff::with_max_skip(64);

        let stats = RunAllProcessorsStats {
            stats: vec![
                make_stat("hs_ok", ProcessorRunStatus::Ok),
                make_stat("hs_err", ProcessorRunStatus::Error),
                make_stat("hs_timeout", ProcessorRunStatus::Timeout),
                make_stat("hs_panic", ProcessorRunStatus::Panic),
            ],
        };

        b.update_from_stats(&stats);

        let all = vec![
            "hs_ok".to_string(),
            "hs_err".to_string(),
            "hs_timeout".to_string(),
            "hs_panic".to_string(),
        ];
        let available = b.filter_backed_off(all);
        assert_eq!(available, vec!["hs_ok"]);
    }

    #[test]
    fn test_unknown_hs_not_filtered() {
        let b = HsBackoff::new();

        // Homeservers with no recorded history should pass through
        let result = b.filter_backed_off(vec!["new_hs".into()]);
        assert_eq!(result, vec!["new_hs"]);
    }

    #[test]
    fn test_failed_to_build_triggers_backoff() {
        let b = HsBackoff::with_max_skip(64);

        let stats = RunAllProcessorsStats {
            stats: vec![make_stat("hs1", ProcessorRunStatus::FailedToBuild)],
        };
        b.update_from_stats(&stats);

        assert!(b.filter_backed_off(vec!["hs1".into()]).is_empty());
    }
}
