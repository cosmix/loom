//! Monitor module for the loom orchestrator
//!
//! Polls `.work/` state files to detect stage completion, context exhaustion,
//! and session crashes. Enables event-driven orchestration without tight coupling.

mod config;
mod context;
pub mod core;
pub(crate) mod detection;
mod events;
pub(crate) mod handlers;

#[cfg(test)]
mod tests;

pub use config::MonitorConfig;
pub use context::{context_health, context_usage_percent, ContextHealth};
pub use core::Monitor;
pub use events::MonitorEvent;
