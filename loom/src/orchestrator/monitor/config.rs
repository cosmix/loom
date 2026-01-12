//! Configuration for the monitor module

use std::path::PathBuf;
use std::time::Duration;

use super::heartbeat::DEFAULT_HUNG_TIMEOUT_SECS;
use crate::models::constants::{CONTEXT_CRITICAL_THRESHOLD, CONTEXT_WARNING_THRESHOLD};

/// Configuration for the monitor
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub poll_interval: Duration,
    pub work_dir: PathBuf,
    pub context_warning_threshold: f32,
    pub context_critical_threshold: f32,
    /// Timeout for considering a session hung (no heartbeat)
    pub hung_timeout: Duration,
    /// Maximum consecutive failures before escalating
    pub max_failures_before_escalation: u32,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            work_dir: PathBuf::from(".work"),
            context_warning_threshold: CONTEXT_WARNING_THRESHOLD,
            context_critical_threshold: CONTEXT_CRITICAL_THRESHOLD,
            hung_timeout: Duration::from_secs(DEFAULT_HUNG_TIMEOUT_SECS),
            max_failures_before_escalation: 3,
        }
    }
}
