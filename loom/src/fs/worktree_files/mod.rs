//! Legacy worktree-related file operations.
//!
//! Retained for compatibility. This cleanup is not Loom's stage retirement
//! flow and does not handle process liveness, final relay drain, settings, or
//! memory. New session lookups should use [`crate::fs::session_files`].
//!
//! ## File Types
//!
//! - **Session files** (`.loom/work/sessions/{session-id}.md`) - Track active sessions
//! - **Signal files** (`.loom/work/signals/{session-id}.md`) - Assignment signals for agents
//! - **Stage files** (`.loom/work/stages/{depth}-{stage-id}.md`) - Stage definitions and status
//! - **Handoff files** (`.loom/work/handoffs/...`) - Context handoffs between sessions

mod cleanup;
mod config;
mod sessions;
mod signals;
mod stages;

#[cfg(test)]
#[allow(deprecated)]
mod tests;

// Re-export public types and functions
#[deprecated(
    note = "legacy cleanup does not perform stage retirement; use the current retirement flow"
)]
pub use cleanup::cleanup_stage_files;
#[deprecated(
    note = "legacy cleanup does not perform stage retirement; use the current retirement flow"
)]
pub use config::{StageFileCleanupConfig, StageFileCleanupResult};
pub use sessions::find_sessions_for_stage;
#[deprecated(
    note = "legacy cleanup does not perform stage retirement; use the current retirement flow"
)]
pub use sessions::remove_session_file;
#[deprecated(
    note = "legacy cleanup does not perform stage retirement; use the current retirement flow"
)]
pub use signals::remove_signal_file;
#[deprecated(
    note = "legacy cleanup does not perform stage retirement; use the current retirement flow"
)]
pub use stages::stage_has_files;

// Re-export canonical stage file finder from fs/stage_files
pub use crate::fs::stage_files::find_stage_file;
