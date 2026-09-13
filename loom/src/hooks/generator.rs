//! Loom hooks directory discovery.
//!
//! Session settings (including hooks) are built into each session's capsule
//! (`orchestrator/terminal/native/session_settings.rs`); this module only
//! locates the installed hook scripts on the host.

/// Find the loom hooks directory
///
/// Looks for hooks in:
/// 1. `$LOOM_HOOKS_DIR` environment variable (for testing/override)
/// 2. `~/.claude/hooks/loom/` (standard installation location)
///
/// Returns None if hooks are not installed. Run `loom init` to install hooks.
pub fn find_hooks_dir() -> Option<std::path::PathBuf> {
    // Check environment variable first (for testing/override)
    if let Ok(dir) = std::env::var("LOOM_HOOKS_DIR") {
        let path = std::path::PathBuf::from(dir);
        if path.exists() {
            return Some(path);
        }
    }

    // Check standard installation location: ~/.claude/hooks/loom/
    if let Some(home_dir) = dirs::home_dir() {
        let installed_hooks = home_dir.join(".claude/hooks/loom");
        if installed_hooks.exists() {
            return Some(installed_hooks);
        }
    }

    None
}
