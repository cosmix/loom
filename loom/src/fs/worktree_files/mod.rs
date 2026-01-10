//! Worktree-related file operations
//!
//! This module provides utilities for managing files associated with worktrees
//! and stages in the `.work/` directory. It handles cleanup of session files,
//! signal files, and other stage-related metadata after merge operations.
//!
//! ## File Types
//!
//! - **Session files** (`.work/sessions/{session-id}.md`) - Track active sessions
//! - **Signal files** (`.work/signals/{session-id}.md`) - Assignment signals for agents
//! - **Stage files** (`.work/stages/{depth}-{stage-id}.md`) - Stage definitions and status
//! - **Handoff files** (`.work/handoffs/...`) - Context handoffs between sessions

mod cleanup;
mod config;
mod sessions;
mod signals;
mod stages;

#[cfg(test)]
mod tests;

// Re-export public types and functions
pub use cleanup::cleanup_stage_files;
pub use config::{StageFileCleanupConfig, StageFileCleanupResult};
pub use sessions::{find_sessions_for_stage, remove_session_file};
pub use signals::remove_signal_file;
pub use stages::{find_stage_file_by_id, stage_has_files};
