//! Claude Code permissions management for loom
//!
//! Manages two settings files:
//! - `.claude/settings.json` - team-shared permissions (committed to git)
//! - `.claude/settings.local.json` - user-local hooks and env vars (gitignored)

pub mod constants;
mod hooks;
pub(crate) mod settings;
mod sync;
mod trust;

#[cfg(test)]
mod tests;

// Re-export public API
pub use constants::{LOOM_PERMISSIONS, LOOM_PERMISSIONS_WORKTREE};
pub use hooks::{get_installed_hooks_dir, install_loom_hooks, loom_hooks_config};
pub use settings::{
    ensure_loom_hooks_local, ensure_loom_permissions, settings_json_has_hooks,
    settings_local_has_hooks,
};
pub use sync::{sync_worktree_permissions, sync_worktree_permissions_with_working_dir, SyncResult};
pub use trust::{migrate_legacy_trust, trust_worktree, untrust_worktree};
