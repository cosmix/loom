//! Settings file management for loom permissions
//!
//! # Settings Files
//!
//! Loom uses two settings files in `.claude/`:
//!
//! ## `settings.json` - Team-shared permissions (committed to git)
//!
//! Contains generic permissions that apply to all Claude Code sessions in the project.
//! This file is safe to commit because it contains no user-specific paths.
//!
//! - **Permissions**: File access rules (e.g., `Read(.loom/work/**)`, `Bash(loom *)`)
//!
//! ## `settings.local.json` - User-local hooks and env (gitignored)
//!
//! Contains hooks and environment variables that reference user-specific paths
//! (e.g., `~/.claude/hooks/loom/`). This file is NOT committed to the repository.
//!
//! - **Hooks**: Global event-triggered scripts (e.g., `commit-guard.sh`, `ask-user-pre.sh`)
//! - **Env**: `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` and other loom env vars
//!
//! Created/updated by `loom init`. Worktrees merge this with session-specific hooks at creation time.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

use super::constants::LOOM_PERMISSIONS;
use super::hooks::{configure_loom_hooks, install_loom_hooks, install_loom_hooks_to};
use super::write_rules::{
    heal_inert_write_denies, prune_legacy_work_write_grants, prune_stale_token_denies,
};
use crate::fs::locking::locked_write;

/// Read a settings file into a JSON value, creating its `.claude` directory and
/// defaulting to an empty object when the file does not exist yet.
fn load_settings_or_default(settings_path: &Path) -> Result<Value> {
    if let Some(claude_dir) = settings_path.parent() {
        if !claude_dir.exists() {
            fs::create_dir_all(claude_dir).with_context(|| {
                format!(
                    "Failed to create .claude directory at {}",
                    claude_dir.display()
                )
            })?;
        }
    }
    if !settings_path.exists() {
        return Ok(json!({}));
    }
    let content = fs::read_to_string(settings_path)
        .with_context(|| format!("Failed to read {}", settings_path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {} as JSON", settings_path.display()))
}

/// Borrow a JSON value as a mutable object map, or error with a labeled message.
fn require_object<'a>(value: &'a mut Value, label: &str) -> Result<&'a mut Map<String, Value>> {
    value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{label} must be a JSON object"))
}

/// Home-expanded twin of `LOOM_PERMISSIONS`'s static
/// `Bash(~/.claude/hooks/loom/codex-forward.sh:*)` allow entry.
///
/// `hooks/codex-forward-guard.sh` accepts TWO spellings of the wrapper path —
/// the literal `~/...` form and the fully `$HOME`-expanded absolute form (see
/// `is_exact_forward_command` there, and `hooks/tests/codex-forward-guard-quoting.sh`,
/// which pins the absolute form deliberately). A forwarder subagent that
/// writes the absolute spelling passes the guard but was still denied by the
/// Claude Code permission classifier, since only the `~` spelling could live
/// in `LOOM_PERMISSIONS` — `$HOME` varies per machine, so a `&'static str`
/// can't express it. This computes that second spelling at runtime, matching
/// the `dirs::home_dir()` pattern already used for hook installation (see
/// `hooks.rs`).
///
/// Returns `None` if the home directory can't be determined; callers must
/// treat that as "skip the dynamic entry" rather than a hard failure — a
/// machine without a resolvable home dir must not lose the static entry too.
///
/// `pub(crate)`: `settings` is already a `pub(crate) mod` (see
/// `permissions/mod.rs`), so any other in-crate call site that ever needs
/// this same entry (e.g. a future worktree-side fold) can reuse it directly
/// instead of re-deriving the string.
pub(crate) fn codex_forward_home_allow_entry() -> Option<String> {
    let home = dirs::home_dir()?;
    Some(format!(
        "Bash({}/.claude/hooks/loom/codex-forward.sh:*)",
        home.display()
    ))
}

/// Per-session identity env vars that must NEVER be persisted in settings files.
///
/// These are set dynamically by the session wrapper script (`export LOOM_...`
/// before `exec claude`) so they always reflect the actual running session.
/// Settings-file `env` blocks override the process environment, so a persisted
/// value from an earlier session silently shadows the wrapper's fresh exports:
/// `loom memory` files entries under the wrong stage, hooks heartbeat the wrong
/// session, and commit-filter misidentifies the main agent.
pub const SESSION_IDENTITY_ENV_KEYS: &[&str] =
    &["LOOM_MAIN_AGENT_PID", "LOOM_STAGE_ID", "LOOM_SESSION_ID"];

/// Remove per-session identity env vars from a settings document.
///
/// Returns `true` if any key was removed. Missing or non-object `env` blocks
/// are left untouched.
pub fn scrub_session_identity_env(settings: &mut Value) -> bool {
    let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let mut removed = false;
    for key in SESSION_IDENTITY_ENV_KEYS {
        removed |= env.remove(*key).is_some();
    }
    removed
}

/// Env var name for the pinned `.loom/work` directory path.
const WORK_DIR_ENV_KEY: &str = "LOOM_WORK_DIR";

/// Remove a stale `LOOM_WORK_DIR` pin from a settings document.
///
/// `LOOM_WORK_DIR` is deliberately excluded from `SESSION_IDENTITY_ENV_KEYS`
/// above: it is repo-stable rather than per-session, so a *live* pin (naming
/// a `.loom/work/` that still exists) is worth persisting — it saves every
/// hook and CLI invocation in the repo the upward search `WorkDir::new` would
/// otherwise perform, and callers reading it while a stage is running rely
/// on it being there. But once the directory it names is gone — e.g. `loom
/// repair --fix` deleted a corrupted `.loom/work/` — the pin is strictly worse
/// than having none at all: Claude Code's settings `env` block overrides the
/// process environment, so the stale value shadows `WorkDir::new`'s upward
/// search instead of falling through to it, and hooks/CLI invocations in the
/// repo silently resolve nothing until someone notices and hand-edits the
/// settings file.
///
/// Returns `true` if the key was removed. A missing/non-object `env` block,
/// a missing key, or a key whose path still exists on disk are all left
/// untouched.
pub fn scrub_stale_work_dir_env(settings: &mut Value) -> bool {
    let Some(env) = settings.get_mut("env").and_then(|v| v.as_object_mut()) else {
        return false;
    };
    let Some(work_dir) = env.get(WORK_DIR_ENV_KEY).and_then(|v| v.as_str()) else {
        return false;
    };
    if Path::new(work_dir).exists() {
        return false;
    }
    env.remove(WORK_DIR_ENV_KEY);
    true
}

/// Heal the MAIN repo's settings files of stale per-session identity env and
/// a stale `LOOM_WORK_DIR` pin.
///
/// Claude Code applies the main repository's settings env to sessions running
/// in linked worktrees, so stale identity in either main-repo settings file
/// shadows the wrapper script's fresh exports in EVERY session of this repo —
/// worktree stages included. Scrubbing the worktree-side copies is therefore
/// not enough; the main files must be healed in the run path, not only on
/// `loom init`/`loom repair` (which polluted repos may never re-run). The two
/// heals travel together here — see [`scrub_session_identity_env`] and
/// [`scrub_stale_work_dir_env`] for what each removes and why.
///
/// Best-effort: missing or unparseable files are skipped. Returns the paths
/// that were healed.
pub fn scrub_main_repo_settings_identity(repo_root: &Path) -> Vec<std::path::PathBuf> {
    let mut healed = Vec::new();
    for name in ["settings.json", "settings.local.json"] {
        let path = repo_root.join(".claude").join(name);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut settings) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let identity_removed = scrub_session_identity_env(&mut settings);
        let stale_work_dir_removed = scrub_stale_work_dir_env(&mut settings);
        if !identity_removed && !stale_work_dir_removed {
            continue;
        }
        let Ok(updated) = serde_json::to_string_pretty(&settings) else {
            continue;
        };
        if locked_write(&path, &updated).is_ok() {
            healed.push(path);
        }
    }
    healed
}

/// Ensure `.claude/settings.json` has loom permissions configured
///
/// This function:
/// 1. Installs loom hook scripts to ~/.claude/hooks/loom/
/// 2. Creates `.claude/` directory if it doesn't exist
/// 3. Creates `settings.json` if it doesn't exist
/// 4. Merges loom permissions into existing file without duplicates
/// 5. Migrates hooks/env from settings.json to settings.local.json (if present)
/// 6. Writes hooks + env to settings.local.json
///
/// Worktrees will merge this config with session-specific hooks at creation time.
pub fn ensure_loom_permissions(repo_root: &Path) -> Result<()> {
    ensure_loom_permissions_to(repo_root, None)
}

/// Testable variant: pass `Some(dir)` to redirect hook installation to a temp directory.
/// Production callers use `ensure_loom_permissions` which passes `None` (installs to ~/.claude/hooks/loom/).
pub fn ensure_loom_permissions_to(repo_root: &Path, hooks_dir: Option<&Path>) -> Result<()> {
    ensure_loom_permissions_inner(repo_root, hooks_dir, true)
}

/// Quiet variant of [`ensure_loom_permissions_to`] for `loom init`'s
/// unattended workspace-repair pass: does the identical work but prints
/// nothing, so `loom init` can render its own single "Repaired: ..." line
/// instead of these diagnostics.
pub fn ensure_loom_permissions_quiet(repo_root: &Path) -> Result<()> {
    ensure_loom_permissions_inner(repo_root, None, false)
}

fn ensure_loom_permissions_inner(
    repo_root: &Path,
    hooks_dir: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    // Install loom hooks
    let hooks_installed = match hooks_dir {
        Some(dir) => install_loom_hooks_to(dir)?,
        None => install_loom_hooks()?,
    };
    if verbose && hooks_installed > 0 {
        println!("  Installed {hooks_installed} loom hook(s)");
    }

    let settings_path = repo_root.join(".claude").join("settings.json");
    let mut settings = load_settings_or_default(&settings_path)?;

    // Ensure settings is an object
    let settings_obj = require_object(&mut settings, "settings.json")?;

    // Get or create permissions object
    let permissions = settings_obj
        .entry("permissions")
        .or_insert_with(|| json!({}));

    let permissions_obj = require_object(permissions, "permissions")?;

    // Get or create allow array
    let allow = permissions_obj.entry("allow").or_insert_with(|| json!([]));

    let allow_arr = allow
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("permissions.allow must be a JSON array"))?;

    let removed_permissions = prune_legacy_work_write_grants(allow_arr);

    // Collect existing permissions as strings for deduplication
    let existing: std::collections::HashSet<String> = allow_arr
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    // Add missing loom permissions
    let mut added_permissions = 0;
    for permission in LOOM_PERMISSIONS {
        if !existing.contains(*permission) {
            allow_arr.push(json!(permission));
            added_permissions += 1;
        }
    }

    // Additive: also allow the home-expanded spelling of the codex forwarding
    // wrapper (see `codex_forward_home_allow_entry` for why this can't live
    // in the static LOOM_PERMISSIONS array above). Skipped silently if the
    // home directory can't be resolved — never fail the whole permission
    // write over it.
    if let Some(home_entry) = codex_forward_home_allow_entry() {
        if !existing.contains(home_entry.as_str()) {
            allow_arr.push(json!(home_entry));
            added_permissions += 1;
        }
    }

    // Migrate: remove hooks and env from settings.json (they belong in settings.local.json)
    let migrated = migrate_hooks_to_local(settings_obj);

    // Write back if we made any changes
    if added_permissions > 0 || removed_permissions > 0 || migrated {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;

        locked_write(&settings_path, &content)
            .with_context(|| format!("Failed to write {}", settings_path.display()))?;

        if verbose {
            if added_permissions > 0 {
                println!(
                    "  Updated .claude/settings.json with {added_permissions} loom permission(s)"
                );
            }
            if removed_permissions > 0 {
                println!("  Removed {removed_permissions} inert .loom/work write grant(s)");
            }
            if migrated {
                println!(
                    "  Migrated hooks/env from .claude/settings.json to .claude/settings.local.json"
                );
            }
        }
    } else if verbose {
        println!("  Claude Code permissions already configured");
    }

    // Write hooks and env to settings.local.json
    ensure_loom_hooks_local_inner(repo_root, verbose)?;

    Ok(())
}

/// Write loom hooks and env vars to `.claude/settings.local.json`
///
/// This merges loom hooks and environment variables into the existing
/// settings.local.json (which may already contain sandbox config and
/// runtime permissions). User-specific paths in hooks make this file
/// unsuitable for committing to git.
pub fn ensure_loom_hooks_local(repo_root: &Path) -> Result<()> {
    ensure_loom_hooks_local_inner(repo_root, true)
}

fn ensure_loom_hooks_local_inner(repo_root: &Path, verbose: bool) -> Result<()> {
    let settings_local_path = repo_root.join(".claude").join("settings.local.json");
    let mut settings = load_settings_or_default(&settings_local_path)?;

    // Ensure settings is an object
    let settings_obj = require_object(&mut settings, "settings.local.json")?;

    // Configure hooks
    let hooks_configured = configure_loom_hooks(settings_obj)?;

    // Configure agent teams environment variable
    let env_map = super::drift::coerced_object(settings_obj, "env")?;
    let env_configured = if !env_map.contains_key("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS") {
        env_map.insert(
            "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS".to_string(),
            json!("1"),
        );
        true
    } else {
        false
    };

    // Drop stale per-session identity env vars left behind by older loom
    // versions (they used to be written here by knowledge-stage spawns and
    // would shadow the wrapper script's fresh exports in every later session),
    // and a LOOM_WORK_DIR pin left behind by a since-deleted .loom/work/ (e.g. a
    // `loom repair --fix` run). Both heals are reported as one change below.
    let stale_env_removed =
        scrub_session_identity_env(&mut settings) | scrub_stale_work_dir_env(&mut settings);
    let settings_obj = require_object(&mut settings, "settings.local.json")?;

    // Disable Claude Code's worktree isolation for subagents in the main repo.
    //
    // Knowledge stages (and interactive sessions) run in the main checkout
    // rather than a loom worktree. With Claude Code's default bgIsolation
    // ("worktree"), their subagents would be forced into nested git worktrees,
    // leaving stray branches behind. "none" lets subagents edit the checkout
    // directly. Worktree stage sessions get the same setting via the sandbox
    // settings generator. (Claude Code v2.1.143+; older versions ignore it.)
    let worktree_map = super::drift::coerced_object(settings_obj, "worktree")?;
    let worktree_configured = if worktree_map.get("bgIsolation") != Some(&json!("none")) {
        worktree_map.insert("bgIsolation".to_string(), json!("none"));
        true
    } else {
        false
    };

    let codex_configured = super::codex_sandbox::merge_allowances(settings_obj);

    // Heal deny rules an older loom left here — inert `Write(...)` spellings
    // and token denies in the shape that makes every `rg`/`grep` prompt.
    let denies_migrated =
        heal_inert_write_denies(settings_obj) | prune_stale_token_denies(settings_obj);

    let changes = [
        (hooks_configured, "Configured loom hooks"),
        (env_configured, "Configured agent teams env var"),
        (worktree_configured, "Disabled worktree isolation"),
        (codex_configured, "Granted codex + cache sandbox access"),
        (stale_env_removed, "Removed stale session env vars"),
        (denies_migrated, "Healed stale deny rules"),
    ];

    // Write back if we made any changes
    if changes.iter().any(|(changed, _)| *changed) {
        let content = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings.local.json to JSON")?;

        locked_write(&settings_local_path, &content)
            .with_context(|| format!("Failed to write {}", settings_local_path.display()))?;

        if verbose {
            for (_, change) in changes.iter().filter(|(changed, _)| *changed) {
                println!("  {change} in .claude/settings.local.json");
            }
        }
    } else if verbose {
        println!("  Hooks and env vars already configured in .claude/settings.local.json");
    }

    Ok(())
}

/// Migrate hooks and env from settings.json to settings.local.json
///
/// If settings.json contains a `hooks` key or `env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`,
/// remove them (they will be recreated in settings.local.json by `ensure_loom_hooks_local`).
///
/// Returns true if any migration was performed.
fn migrate_hooks_to_local(settings_obj: &mut serde_json::Map<String, Value>) -> bool {
    let mut migrated = false;

    // Remove hooks from settings.json
    if settings_obj.remove("hooks").is_some() {
        migrated = true;
    }

    // Remove CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS and stale per-session
    // identity from env in settings.json (very old loom versions persisted
    // identity here; it shadows the wrapper's exports in every session)
    if let Some(env) = settings_obj.get_mut("env").and_then(|v| v.as_object_mut()) {
        if env.remove("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS").is_some() {
            migrated = true;
        }
        for key in SESSION_IDENTITY_ENV_KEYS {
            if env.remove(*key).is_some() {
                migrated = true;
            }
        }
        // If env is now empty, remove it entirely
        if env.is_empty() {
            settings_obj.remove("env");
        }
    }

    migrated
}

/// Check if settings.json still contains hooks that should be in settings.local.json
pub fn settings_json_has_hooks(repo_root: &Path) -> bool {
    let settings_path = repo_root.join(".claude/settings.json");
    if !settings_path.exists() {
        return false;
    }

    let content = match fs::read_to_string(&settings_path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    let settings: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    settings.get("hooks").is_some()
}

#[cfg(test)]
mod tests;
