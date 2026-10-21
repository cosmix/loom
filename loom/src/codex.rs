//! Shared Codex binary resolution utilities.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// Codex model for common implementation and integration tests (sonnet's peer tier).
pub const CODEX_IMPLEMENTER_MODEL_TERRA: &str = "gpt-5.6-terra";

/// Codex model for boilerplate, scaffolding, and simple unit tests.
pub const CODEX_IMPLEMENTER_MODEL_LUNA: &str = "gpt-6-luna";

/// Reasoning effort used for Codex implementation runs.
pub const CODEX_IMPLEMENTER_EFFORT: &str = "xhigh";

/// Codex models `loom pressure` accepts. Kept in step with the forwarding
/// hooks' allowlists in `loom-hooks/codex-forward.sh`,
/// `codex-forward-guard.sh` and `codex-forward-result.sh`. Ids follow
/// `gpt-<version>-<family>`, and `loom pressure` falls back within a family
/// when the account cannot use the requested model (see
/// [`codex_model_candidates`]); tiers never mix.
pub const CODEX_MODELS: &[&str] = &[
    "gpt-6-astra",
    "gpt-6.1-sol",
    "gpt-6-sol",
    "gpt-5.6-terra",
    "gpt-6-luna",
];

/// Codex model the pressure run defaults to.
pub const DEFAULT_PRESSURE_CODEX_MODEL: &str = "gpt-6.1-sol";

/// The models `loom pressure` tries, in order, for a requested codex model:
/// the requested one first, then the rest of its family from
/// [`CODEX_MODELS`] — newer versions ascending (nearest newer first), then
/// older versions descending (nearest older first). A newly announced model
/// is often not yet enabled for every account, so its siblings stand in for
/// it. An id outside [`CODEX_MODELS`] gets no fallbacks.
pub fn codex_model_candidates(requested: &str) -> Vec<&str> {
    let Some((version, family)) = parse_codex_model(requested) else {
        return vec![requested];
    };
    if !CODEX_MODELS.contains(&requested) {
        return vec![requested];
    }
    let (mut newer, mut older): (Vec<_>, Vec<_>) = family_members(family)
        .into_iter()
        .filter(|(member, _)| compare_versions(member, &version).is_ne())
        .partition(|(member, _)| compare_versions(member, &version).is_gt());
    newer.sort_by(|a, b| compare_versions(&a.0, &b.0));
    older.sort_by(|a, b| compare_versions(&b.0, &a.0));
    std::iter::once(requested)
        .chain(newer.into_iter().chain(older).map(|(_, id)| id))
        .collect()
}

/// Every [`CODEX_MODELS`] entry of `family`, paired with its parsed version,
/// so [`codex_model_candidates`] can order siblings without reparsing.
fn family_members(family: &str) -> Vec<(Vec<u32>, &'static str)> {
    CODEX_MODELS
        .iter()
        .filter_map(|&id| parse_codex_model(id).map(|(version, fam)| (version, fam, id)))
        .filter(|(_, fam, _)| *fam == family)
        .map(|(version, _, id)| (version, id))
        .collect()
}

/// Split `gpt-<version>-<family>` into its dotted numeric version segments
/// and its family. `None` for an id outside that shape, which then never
/// gains fallbacks.
fn parse_codex_model(id: &str) -> Option<(Vec<u32>, &str)> {
    let (version, family) = id.strip_prefix("gpt-")?.split_once('-')?;
    if family.is_empty() {
        return None;
    }
    let segments = version
        .split('.')
        .map(|segment| segment.parse().ok())
        .collect::<Option<Vec<u32>>>()?;
    Some((segments, family))
}

/// Compare dotted versions segment-wise, a missing segment counting as 0, so
/// `6 < 6.1`, `5.6 < 6` and `6 == 6.0` — the order the fallback chain needs.
fn compare_versions(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let segment = |v: &[u32], i: usize| v.get(i).copied().unwrap_or(0);
    (0..a.len().max(b.len()))
        .map(|i| segment(a, i).cmp(&segment(b, i)))
        .find(|order| order.is_ne())
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Reasoning efforts the Codex CLI accepts, cheapest first. `loom pressure`
/// validates `--codex-effort` and `pressure.codex_effort` against this list
/// because the value reaches the command line as
/// `-c model_reasoning_effort=<value>`.
pub const CODEX_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh"];

/// Codex reasoning effort the pressure run defaults to.
pub const DEFAULT_PRESSURE_CODEX_EFFORT: &str = "xhigh";

/// Paths the codex lane must be able to WRITE from inside the Bash sandbox.
///
/// Codex is a subprocess, not a Claude tool, and it keeps its state outside the
/// worktree: the CLI initialises a sqlite state runtime plus session logs under
/// `~/.codex`, and the plugin's companion runtime records each job under
/// `~/.claude/plugins/data/codex-openai-codex/state/<cwd>-<hash>/jobs/`. The
/// sandbox's write set is otherwise the working directory and the session temp
/// dir only, so without these two entries every forward dies before the model
/// is ever reached — `Read-only file system (os error 30)` from codex, `ENOENT:
/// ... mkdir` from the companion. This bit on Linux first: the native
/// bubblewrap sandbox enforces the write allowlist that macOS Seatbelt let pass.
///
/// This is emitted as `sandbox.filesystem.allowWrite`, which is ADDITIVE
/// ("additional paths to allow writing within the sandbox") and OS-enforced for
/// child processes — the one lever that reaches a subprocess. Do NOT "fix" a
/// blocked codex run with `dangerouslyDisableSandbox` instead: that retry goes
/// back through the permission gate, and the auto-mode classifier refuses it, so
/// the lane ends up unusable rather than merely sandboxed.
pub const CODEX_SANDBOX_WRITE_PATHS: [&str; 2] = ["~/.codex", CODEX_PLUGIN_DATA_GRANT];

/// The plugin companion's state directory: the one path under
/// `~/.claude/plugins` the codex lane may write. Every capsule that licenses
/// the lane denies each other entry on the way down to it.
pub const CODEX_PLUGIN_DATA_GRANT: &str = "~/.claude/plugins/data/codex-openai-codex";

/// Domains the codex CLI reaches to run a task (ChatGPT-login and API auth).
///
/// The sandbox pre-allows no domains at all, so an unlisted host raises a
/// permission decision mid-run — which in a headless stage lands on the same
/// auto-mode classifier that blocks the sandbox escape. Pre-allowing them keeps
/// a codex forward from stalling on its first network call.
pub const CODEX_SANDBOX_DOMAINS: [&str; 4] = [
    "chatgpt.com",
    "*.chatgpt.com",
    "api.openai.com",
    "auth.openai.com",
];

/// `~/.codex/config.toml` — the codex CLI's user configuration file.
///
/// This is the only sandbox-configuration channel loom controls for the lane:
/// the forwarder's one Bash call goes through the plugin's companion runtime,
/// which spawns `codex app-server` itself, so no `-c` CLI override can be
/// threaded through a forward.
///
/// That holds for the companion path. Where the outer sandbox refuses the
/// nesting (a nested Seatbelt on macOS, a nested bubblewrap on Linux when
/// `bwrap` is on PATH), `loom-hooks/codex-forward.sh` bypasses the companion and
/// passes `--sandbox danger-full-access` to `codex exec` itself; this file then
/// plays no part in the run.
pub fn codex_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".codex/config.toml"))
}

/// Whether codex's own `workspace-write` sandbox excludes `/tmp`.
///
/// Codex is not just sandboxed BY the Bash sandbox — it brings its own nested
/// bubblewrap sandbox, and by default `workspace-write` claims `/tmp` as a
/// writable root and masks `.git` under every writable root. `/tmp/.git` does
/// not exist, so bwrap must create that mountpoint; the outer sandbox keeps
/// `/tmp` read-only (only `/tmp/claude` and `$TMPDIR` are writable), so every
/// sandboxed codex exec dies at namespace setup with `bwrap: Can't mkdir
/// /tmp/.git: Read-only file system` before the model runs a single command.
/// On macOS the nesting is refused altogether (`sandbox-exec: sandbox_apply:
/// Operation not permitted`) and the wrapper's direct `codex exec` mode is the
/// fix there, so this key matters only where the nesting works: Linux.
///
/// `sandbox_workspace_write.exclude_slash_tmp = true` removes `/tmp` from the
/// writable roots (the session `$TMPDIR` remains, and IS writable in the outer
/// sandbox), which is the whole fix — verified empirically with
/// `codex sandbox -c sandbox_mode="workspace-write" -- echo hi` inside a stage
/// sandbox. Widening the outer sandbox with `allowWrite: /tmp` instead would
/// be both broader than needed and actively hazardous: bwrap's mountpoint
/// mkdir writes through to the host, and a stray `/tmp/.git` makes git
/// discovery under any `/tmp` directory find a phantom repository.
pub fn codex_config_excludes_slash_tmp(config_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(doc) = content.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    doc.get("sandbox_workspace_write")
        .and_then(|table| table.get("exclude_slash_tmp"))
        .and_then(|item| item.as_bool())
        .unwrap_or(false)
}

/// Set `sandbox_workspace_write.exclude_slash_tmp = true` in codex's config.
///
/// Returns `Ok(true)` when the file was changed, `Ok(false)` when it already
/// carried the exclusion. Comments and unrelated keys are preserved
/// (`toml_edit`); an unparseable file is an error, never rewritten.
pub fn ensure_codex_config_excludes_slash_tmp(config_path: &Path) -> Result<bool> {
    if codex_config_excludes_slash_tmp(config_path) {
        return Ok(false);
    }
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(err).with_context(|| format!("failed to read {}", config_path.display()));
        }
    };
    let mut doc = content.parse::<toml_edit::DocumentMut>().with_context(|| {
        format!(
            "refusing to rewrite unparseable codex config at {}",
            config_path.display()
        )
    })?;
    let table = doc
        .entry("sandbox_workspace_write")
        .or_insert(toml_edit::table());
    table["exclude_slash_tmp"] = toml_edit::value(true);
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(config_path, doc.to_string())
        .with_context(|| format!("failed to write {}", config_path.display()))?;
    Ok(true)
}

/// Sentinel that MUST be the first line of every codex-lane subagent prompt.
///
/// `loom-hooks/codex-forward-guard.sh` greps the calling subagent's transcript for
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
pub fn codex_lane_status() -> Result<()> {
    if find_codex_path().is_err() {
        bail!("codex CLI not found in PATH or common install locations");
    }
    if !codex_companion_installed() {
        bail!(
            "codex plugin companion runtime not found under ~/.claude/plugins/cache/openai-codex/"
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

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
