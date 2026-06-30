//! Shared Codex binary resolution utilities.

use anyhow::{bail, Result};
use std::path::PathBuf;

/// Find the absolute path to the codex binary.
///
/// Mirrors [`crate::claude::find_claude_path`]: try `which::which` first (uses
/// the current PATH), then fall back to a fixed list of common install
/// locations. Spawned terminals/children may not inherit the parent's PATH, so
/// resolve eagerly. The candidate list differs from claude's because codex is
/// typically installed via bun/npm rather than the Claude Code installer.
pub fn find_codex_path() -> Result<PathBuf> {
    // First try which::which (uses current PATH)
    if let Ok(path) = which::which("codex") {
        return Ok(path);
    }

    // Common installation locations for the codex CLI.
    let candidates = [
        dirs::home_dir().map(|h| h.join(".bun/bin/codex")),
        dirs::home_dir().map(|h| h.join(".local/bin/codex")),
        dirs::home_dir().map(|h| h.join(".npm-global/bin/codex")),
        dirs::home_dir().map(|h| h.join(".cargo/bin/codex")),
        Some(PathBuf::from("/usr/local/bin/codex")),
        Some(PathBuf::from("/opt/homebrew/bin/codex")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!("codex binary not found in PATH or common locations. Checked: ~/.bun/bin/codex, ~/.local/bin/codex, ~/.npm-global/bin/codex, ~/.cargo/bin/codex, /usr/local/bin/codex, /opt/homebrew/bin/codex")
}
