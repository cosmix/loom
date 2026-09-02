//! Configuration for the monitor module

use std::path::PathBuf;
use std::time::Duration;

use super::heartbeat::DEFAULT_HUNG_TIMEOUT_SECS;
use crate::fs::work_dir::ContextConfig;

/// Configuration for the monitor
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub poll_interval: Duration,
    pub work_dir: PathBuf,
    /// Plan-wide context ceilings, in absolute tokens. Read once from
    /// `.loom/work/config.toml` when the monitor is built; a stage's own
    /// `context_ceiling_tokens` takes precedence over these.
    pub context: ContextConfig,
    /// Timeout for considering a session hung (no heartbeat)
    pub hung_timeout: Duration,
    /// Maximum consecutive failures before escalating
    pub max_failures_before_escalation: u32,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            work_dir: PathBuf::from(".loom/work"),
            context: ContextConfig::default(),
            hung_timeout: Duration::from_secs(DEFAULT_HUNG_TIMEOUT_SECS),
            max_failures_before_escalation: 3,
        }
    }
}
