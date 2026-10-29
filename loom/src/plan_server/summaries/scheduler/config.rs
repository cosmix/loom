//! Configuration and the automatic rebuild budget.

use std::time::Duration;

use super::Scheduler;

/// Summaries rebuilt without asking.
pub const AUTO_LIMIT: usize = 5;
/// How long a summary session may go without a seal before it is stopped.
pub const NO_SEAL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The scheduler's configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Why the server is read-only; `Some` starts and asks nothing.
    pub read_only: Option<String>,
    /// Queued summaries started without asking.
    pub limit: usize,
    /// Time without a seal after which a session is stopped.
    pub no_seal_timeout: Duration,
}

impl SchedulerConfig {
    /// The planning server's rules: [`AUTO_LIMIT`] and [`NO_SEAL_TIMEOUT`].
    pub fn standard(read_only: Option<String>) -> Self {
        SchedulerConfig {
            read_only,
            limit: AUTO_LIMIT,
            no_seal_timeout: NO_SEAL_TIMEOUT,
        }
    }
}

impl Scheduler {
    /// Set the automatic budget; the next queue evaluation uses the new limit.
    pub fn set_auto(&mut self, enabled: bool) {
        self.config.limit = if enabled { AUTO_LIMIT } else { 0 };
    }
}
