//! Claude Code permissions management for loom
//!
//! Ensures that `.claude/settings.json` has the necessary permissions
//! and hooks for loom to operate without constant user approval prompts.

pub mod constants;
mod hooks;
mod settings;
mod sync;
mod trust;

#[cfg(test)]
mod tests;

// Re-export public API
pub use constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
pub use hooks::{get_installed_hooks_dir, install_loom_hooks, loom_hooks_config};
pub use settings::{create_worktree_settings, ensure_loom_permissions};
pub use sync::{sync_worktree_permissions, SyncResult};
pub use trust::add_worktrees_to_global_trust;
