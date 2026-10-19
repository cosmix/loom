//! Loom hooks directory discovery.
//!
//! Session settings (including hooks) are built into each session's capsule
//! (`orchestrator/terminal/native/session_settings.rs`); this module only
//! locates the installed hook scripts on the host.

use std::path::PathBuf;

/// Find the loom hooks directory
///
/// Looks for hooks in:
/// 1. `$LOOM_HOOKS_DIR` environment variable (for testing/override)
/// 2. `<claude_dir>/hooks/loom/`, the Claude root `assets::install::default_paths`
///    resolves (`~/.claude` unless an install override or recorded root moves it)
///
/// A relocated root never falls back to `~/.claude/hooks/loom`: a copy left
/// there by an earlier install is stale, and running it would silently skip
/// this build's hooks.
///
/// Returns None if hooks are not installed. Run `loom init` to install hooks.
pub fn find_hooks_dir() -> Option<PathBuf> {
    let env_override = std::env::var_os("LOOM_HOOKS_DIR").map(PathBuf::from);
    first_existing([env_override, crate::assets::install::claude_hooks_dir()])
}

/// The first of `candidates` that exists on disk.
fn first_existing(candidates: [Option<PathBuf>; 2]) -> Option<PathBuf> {
    candidates.into_iter().flatten().find(|path| path.exists())
}
