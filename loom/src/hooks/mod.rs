//! Claude Code hooks infrastructure for loom orchestrator.
//!
//! This module provides hooks integration that enables:
//! - Auto-handoff on PreCompact (context exhaustion)
//! - Session lifecycle tracking via event logging
//! - Learning validation on Stop (memory usage checks)
//! - Worktree isolation enforcement via PreToolUse hooks
//!
//! ## Hook Events
//!
//! The following Claude Code hook events are supported:
//! - `SessionStart`: Called when a Claude Code session starts
//! - `PostToolUse`: Called after each tool use (heartbeat update)
//! - `PreCompact`: Called before context compaction (triggers handoff)
//! - `SessionEnd`: Called when a session ends normally
//! - `Stop`: Called when session is stopping (learning-validator)
//! - `PreferModernTools`: Called before Bash to suggest modern CLI tools
//! - `WorktreeIsolation`: Called before Bash/Edit/Write to enforce boundaries
//!
//! ## Configuration
//!
//! Hooks are configured via `.claude/settings.json` in each worktree.
//! The hook scripts are located in the loom installation directory under `hooks/`.

mod config;
pub mod events;
mod generator;
pub mod validators;

#[cfg(test)]
mod tests;

pub use config::{HookEvent, HooksConfig};
pub use events::{
    log_hook_event, read_recent_events, read_session_events, read_stage_events, read_tool_events,
    tail_tool_events, HookEventLog, HookEventPayload, ToolEvent,
};
pub use generator::{
    container_main_settings_path, find_hooks_dir, generate_hooks_settings,
    setup_container_main_session_settings, setup_hooks_for_worktree,
};
