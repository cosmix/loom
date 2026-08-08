//! Shared Codex binary resolution utilities.

use anyhow::{bail, Result};
use std::path::PathBuf;

/// Codex model for common implementation and integration tests (sonnet's peer tier).
pub const CODEX_IMPLEMENTER_MODEL_TERRA: &str = "gpt-5.6-terra";

/// Codex model for boilerplate, scaffolding, and simple unit tests.
pub const CODEX_IMPLEMENTER_MODEL_LUNA: &str = "gpt-5.6-luna";

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

/// Whether the codex plugin's companion runtime is installed, for any version.
///
/// Mirrors `agents/loom-codex-forwarder.md`'s COMPANION lookup
/// (`~/.claude/plugins/cache/openai-codex/codex/*/scripts/codex-companion.mjs`)
/// with a plain directory walk instead of a glob dependency - the presence of
/// any version's companion script is enough, so no version sorting is needed.
fn codex_companion_installed() -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    let versions_dir = home.join(".claude/plugins/cache/openai-codex/codex");
    let Ok(entries) = std::fs::read_dir(&versions_dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| entry.path().join("scripts/codex-companion.mjs").is_file())
}

/// Combined availability check for the codex implementation lanes (terra, luna).
///
/// `Ok(())` iff BOTH the codex CLI is resolvable (see [`find_codex_path`]) AND
/// the codex plugin's companion runtime is installed. `Err` carries a short,
/// human-readable reason naming what is missing, for the advisory startup
/// warning and the stage-signal fallback text - this never signals a hard
/// failure, only that the codex lanes are unavailable on this machine.
pub fn codex_lane_status() -> Result<(), String> {
    if find_codex_path().is_err() {
        return Err("codex CLI not found in PATH or common install locations".to_string());
    }
    if !codex_companion_installed() {
        return Err(
            "codex plugin companion runtime not found under ~/.claude/plugins/cache/openai-codex/"
                .to_string(),
        );
    }
    Ok(())
}

/// Memoized [`codex_lane_status`], process-lifetime cached.
///
/// Installation state (the codex CLI and plugin) is invariant for the
/// lifetime of a daemon process, exactly like Remote Control's
/// `cached_preflight_enabled()` (`doc/loom/knowledge/patterns/remote-control.md`).
pub fn codex_lane_available() -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| codex_lane_status().is_ok())
}
