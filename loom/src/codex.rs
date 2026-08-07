//! Shared Codex binary resolution utilities.

use anyhow::{bail, Result};
use std::path::PathBuf;

/// Model used when a stage delegates implementation to Codex.
pub const CODEX_IMPLEMENTER_MODEL: &str = "gpt-5.6-luna";

/// Reasoning effort used for Codex implementation runs.
pub const CODEX_IMPLEMENTER_EFFORT: &str = "xhigh";

/// Sentinel that MUST be the first line of every codex-lane subagent prompt.
///
/// `hooks/codex-forward-guard.sh` greps the calling subagent's transcript for
/// this exact token and, when present, blocks every tool call except the single
/// Bash invocation of codex-companion.mjs - pinning the forwarder to forwarding.
/// The literal in the hook script and in `agents/loom-codex-forwarder.md` must
/// stay byte-identical to this constant; `tests_doctrine.rs` pins all three.
pub const CODEX_FORWARD_SENTINEL: &str = "LOOM-CODEX-FORWARD-ONLY";

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
