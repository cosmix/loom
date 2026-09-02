//! Monitor module for the loom orchestrator
//!
//! Polls `.loom/work/` state files to detect stage completion, context exhaustion,
//! and session crashes. Enables event-driven orchestration without tight coupling.
//!
//! ## Heartbeat Protocol
//!
//! Sessions write heartbeat files to `.loom/work/heartbeat/<stage-id>.json` via hooks.
//! The monitor polls these files to detect:
//! - Crashed sessions (PID dead)
//! - Hung sessions (PID alive but no heartbeat update for threshold duration)

mod budget_latch;
mod ceiling;
mod config;
mod context;
pub mod core;
pub(crate) mod detection;
pub mod events;
pub mod failure_tracking;
pub(crate) mod handlers;
mod handoff_watch;
pub mod heartbeat;
pub(crate) mod hung_latch;
pub(crate) mod parked;
mod session_events;

#[cfg(test)]
mod tests;

pub use config::MonitorConfig;
pub use context::{context_health, ContextHealth};
pub use core::Monitor;
pub use events::MonitorEvent;
pub use failure_tracking::{
    build_failure_info, failure_state_path, FailureRecord, FailureTracker, StageFailureState,
    DEFAULT_MAX_FAILURES,
};
pub use heartbeat::{
    heartbeat_path, read_heartbeat, remove_heartbeat, stage_context_tokens, write_heartbeat,
    Heartbeat, HeartbeatStatus, HeartbeatWatcher, DEFAULT_HEARTBEAT_POLL_SECS,
    DEFAULT_HUNG_TIMEOUT_SECS,
};
