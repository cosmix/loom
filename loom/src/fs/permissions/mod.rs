//! Claude Code permissions management for loom
//!
//! Ensures that `.claude/settings.local.json` has the necessary permissions
//! and hooks for loom to operate without constant user approval prompts.

mod constants;
mod hooks;
mod settings;
mod trust;

#[cfg(test)]
mod tests;

// Re-export public API
pub use constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
pub use hooks::install_loom_hooks;
pub use settings::{create_worktree_settings, ensure_loom_permissions};
pub use trust::add_worktrees_to_global_trust;
