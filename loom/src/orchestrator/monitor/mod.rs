//! Monitor module for the loom orchestrator
//!
//! Polls `.work/` state files to detect stage completion, context exhaustion,
//! and session crashes. Enables event-driven orchestration without tight coupling.
//!
//! ## Heartbeat Protocol
//!
//! Sessions write heartbeat files to `.work/heartbeat/<stage-id>.json` via hooks.
//! The monitor polls these files to detect:
//! - Crashed sessions (PID dead)
//! - Hung sessions (PID alive but no heartbeat update for threshold duration)

pub mod checkpoints;
mod config;
mod context;
pub mod core;
pub(crate) mod detection;
pub mod events;
pub mod failure_tracking;
pub(crate) mod handlers;
pub mod heartbeat;

#[cfg(test)]
mod tests;

pub use checkpoints::{
    generate_correction_guidance, generate_next_task_injection, CheckpointProcessResult,
    CheckpointWatcher, NextTaskInfo,
};
pub use config::MonitorConfig;
pub use context::{context_health, context_usage_percent, ContextHealth};
pub use core::Monitor;
pub use events::MonitorEvent;
pub use failure_tracking::{
    build_failure_info, failure_state_path, FailureRecord, FailureTracker, StageFailureState,
    DEFAULT_MAX_FAILURES,
};
pub use heartbeat::{
    heartbeat_path, read_heartbeat, remove_heartbeat, write_heartbeat, Heartbeat, HeartbeatStatus,
    HeartbeatWatcher, DEFAULT_HEARTBEAT_POLL_SECS, DEFAULT_HUNG_TIMEOUT_SECS,
};
