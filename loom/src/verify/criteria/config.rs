//! Configuration types for acceptance criteria execution

use std::time::Duration;

/// Default timeout for command execution (5 minutes)
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// Configuration for acceptance criteria execution
#[derive(Debug, Clone)]
pub struct CriteriaConfig {
    /// Maximum time to wait for a single command to complete
    pub command_timeout: Duration,
}

impl Default for CriteriaConfig {
    fn default() -> Self {
        Self {
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
        }
    }
}

impl CriteriaConfig {
    /// Create a new configuration with a custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            command_timeout: timeout,
        }
    }
}
