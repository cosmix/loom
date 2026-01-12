//! Claude Code hooks infrastructure for loom orchestrator.
//!
//! This module provides hooks integration that enables:
//! - Auto-handoff on PreCompact (context exhaustion)
//! - Learning protection via Stop hook
//! - Session lifecycle tracking via event logging
//!
//! ## Hook Events
//!
//! The following Claude Code hook events are supported:
//! - `SessionStart`: Called when a Claude Code session starts
//! - `PreCompact`: Called before context compaction (triggers handoff)
//! - `SessionEnd`: Called when a session ends normally
//! - `Stop`: Called when session is stopping (validates learnings)
//! - `SubagentStop`: Called when a subagent stops (extracts learnings)
//!
//! ## Configuration
//!
//! Hooks are configured via `.claude/settings.json` in each worktree.
//! The hook scripts are located in the loom installation directory under `hooks/`.

mod config;
mod events;
mod generator;

#[cfg(test)]
mod tests;

pub use config::{HookEvent, HooksConfig};
pub use events::{log_hook_event, HookEventLog, HookEventPayload};
pub use generator::{generate_hooks_settings, setup_hooks_for_worktree};
