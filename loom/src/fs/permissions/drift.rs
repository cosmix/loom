//! Drift detection for loom's `.claude` configuration.
//!
//! Everything `loom repair` needs to notice that a checkout's settings files
//! or installed hook scripts have fallen out of sync with the canonical loom
//! configuration lives here: hook registration drift (missing/obsolete
//! entries in a settings document's `hooks` block), hook script drift (the
//! scripts installed under `~/.claude/hooks/loom/`), and settings-document
//! drift (agent-teams env, worktree isolation, stale per-session identity).

use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

use super::constants::LOOM_HOOKS;
use super::hooks::{extract_command_basename, is_loom_hook, loom_hooks_config_for_dir};
use super::settings::{scrub_session_identity_env, scrub_stale_work_dir_env};

/// Drift between a settings document's `hooks` block and the canonical loom
/// hook configuration.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HookDrift {
    /// Canonical registrations absent from the settings document, rendered
    /// `"Event:matcher:script.sh"` for reporting.
    pub missing: Vec<String>,
    /// Registrations pointing into a loom hooks directory whose script loom no
    /// longer ships, rendered the same way.
    pub obsolete: Vec<String>,
}

impl HookDrift {
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty() && self.obsolete.is_empty()
    }
}

/// Coerce a loom-owned settings container to the expected JSON shape,
/// replacing a wrong-typed value outright instead of erroring.
///
/// `hooks`, `hooks.<Event>`, `env`, and `worktree` are all containers loom
/// itself manages, and Claude Code rejects any other shape for them — so
/// nothing user-authored can ever be living inside a wrong-typed value there.
/// Erroring on one instead of healing it left `loom repair --fix` with no
/// path back to a healthy file: it reported the same drift on every run and
/// repaired none of it, because the detectors above already treat a
/// wrong-typed container as empty (every canonical entry reports missing)
/// while the fixer used to bail with `Err`. Coercing closes that gap: fixer
/// output converges with what the detector already considers clean.
pub(super) fn coerce_container(slot: &mut Value, empty: fn() -> Value) -> &mut Value {
    let has_expected_shape = match empty() {
        Value::Object(_) => slot.is_object(),
        Value::Array(_) => slot.is_array(),
        _ => true,
    };
    if !has_expected_shape {
        *slot = empty();
    }
    slot
}

/// Get-or-create a loom-owned OBJECT container at `key` within `obj`,
/// coercing a wrong-typed existing value first (see [`coerce_container`]).
///
/// Centralizes the entry/coerce/extract sequence `ensure_loom_hooks_local`
/// needs for both `env` and `worktree`, so those call sites stay one line.
pub(super) fn coerced_object<'a>(
    obj: &'a mut Map<String, Value>,
    key: &str,
) -> anyhow::Result<&'a mut Map<String, Value>> {
    let slot = obj.entry(key).or_insert_with(|| json!({}));
    coerce_container(slot, || json!({}));
    slot.as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{key} must be a JSON object"))
}

/// Flatten a `hooks` block (settings document or canonical config) into
/// `(event, matcher, command)` triples. Malformed shapes are skipped, never
/// treated as an error — a non-array event value or a non-object entry is
/// simply invisible to drift detection.
fn flatten_hook_triples(hooks: &Value) -> Vec<(String, String, String)> {
    let mut triples = Vec::new();
    let Some(hooks_obj) = hooks.as_object() else {
        return triples;
    };
    for (event_name, entries) in hooks_obj {
        let Some(entries) = entries.as_array() else {
            continue;
        };
        for entry in entries {
            let Some(entry_obj) = entry.as_object() else {
                continue;
            };
            let matcher = entry_obj
                .get("matcher")
                .and_then(|m| m.as_str())
                .unwrap_or("");
            let Some(commands) = entry_obj.get("hooks").and_then(|h| h.as_array()) else {
                continue;
            };
            for command in commands {
                let Some(command) = command.get("command").and_then(|c| c.as_str()) else {
                    continue;
                };
                triples.push((event_name.clone(), matcher.to_string(), command.to_string()));
            }
        }
    }
    triples
}

fn render_hook_triple(event: &str, matcher: &str, command: &str) -> String {
    format!("{event}:{matcher}:{command}")
}

/// Compare a settings document's `hooks` block against the canonical loom
/// configuration built for `hooks_dir`.
///
/// Identity of a registration is the exact `(event, matcher, command)` triple
/// — `configure_loom_hooks` writes home-absolute paths, so a tilde-prefixed
/// variant of a genuine registration is real drift, not a false positive (see
/// the `~ may not work in all contexts` comment above); paths are never
/// normalised before comparing.
pub fn hook_drift_for_dir(settings: &Value, hooks_dir: &str) -> HookDrift {
    let canonical = flatten_hook_triples(&loom_hooks_config_for_dir(hooks_dir));
    let actual = settings
        .get("hooks")
        .map(flatten_hook_triples)
        .unwrap_or_default();

    let mut missing: Vec<String> = canonical
        .iter()
        .filter(|(event, matcher, command)| {
            !actual
                .iter()
                .any(|(e, m, c)| e == event && m == matcher && c == command)
        })
        .map(|(event, matcher, command)| render_hook_triple(event, matcher, command))
        .collect();
    missing.sort();

    // Narrowly scoped: only a triple that both points into a loom hooks
    // directory AND names a script loom no longer ships counts as obsolete.
    // Worktree settings files legitimately carry extra registrations
    // (SessionStart / PostToolUse / PreCompact / SessionEnd / Stop, written by
    // `crate::hooks::config::HooksConfig::to_settings_hooks`) whose basenames
    // are all in `LOOM_HOOKS`, so this never flags them.
    let mut obsolete: Vec<String> = actual
        .iter()
        .filter(|(_, _, command)| {
            is_loom_hook(command)
                && !LOOM_HOOKS
                    .iter()
                    .any(|(filename, _)| *filename == extract_command_basename(command))
        })
        .map(|(event, matcher, command)| render_hook_triple(event, matcher, command))
        .collect();
    obsolete.sort();

    HookDrift { missing, obsolete }
}

/// [`hook_drift_for_dir`] against the host hooks directory (`~/.claude/hooks/loom`).
pub fn hook_drift(settings: &Value) -> HookDrift {
    let hooks_dir = dirs::home_dir()
        .map(|h| h.join(".claude/hooks/loom").display().to_string())
        .unwrap_or_else(|| "~/.claude/hooks/loom".to_string());
    hook_drift_for_dir(settings, &hooks_dir)
}

/// Whether an installed hook script matches the embedded copy and is executable.
///
/// Factored out of `install_hook_script` so the installer and
/// [`hook_scripts_needing_install`] share one definition of "current" and can
/// never drift apart on what counts as up to date.
pub(super) fn hook_script_is_current(dir: &Path, filename: &str, content: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;

    let hook_path = dir.join(filename);
    let Ok(existing_content) = fs::read_to_string(&hook_path) else {
        return false;
    };
    if existing_content != content {
        return false;
    }
    let Ok(metadata) = fs::metadata(&hook_path) else {
        return false;
    };
    metadata.permissions().mode() & 0o111 != 0
}

/// Loom hook scripts in `hooks_dir` that are missing, content-drifted, or not
/// executable. Returns their filenames, sorted.
///
/// A script that exists with stale content or without its execute bit fails
/// silently at runtime (Claude Code just never invokes it), so this is not a
/// redundant check on top of the registration drift in [`hook_drift`] — a
/// repo can have every registration correct and every script a dead path.
pub fn hook_scripts_needing_install(hooks_dir: &Path) -> Vec<&'static str> {
    let mut needing: Vec<&'static str> = LOOM_HOOKS
        .iter()
        .filter(|(filename, content)| !hook_script_is_current(hooks_dir, filename, content))
        .map(|(filename, _)| *filename)
        .collect();
    needing.sort_unstable();
    needing
}

/// [`hook_scripts_needing_install`] against the host hooks directory.
/// Returns an empty vec if the home directory cannot be determined.
pub fn loom_hook_scripts_needing_install() -> Vec<&'static str> {
    match super::hooks::get_installed_hooks_dir() {
        Some(dir) => hook_scripts_needing_install(&dir),
        None => Vec::new(),
    }
}

/// Read `.claude/settings.local.json` as JSON, or `None` if absent/unparseable.
fn read_settings_local(repo_root: &Path) -> Option<Value> {
    let content = fs::read_to_string(repo_root.join(".claude/settings.local.json")).ok()?;
    serde_json::from_str(&content).ok()
}

/// Hook drift in the MAIN repo's `.claude/settings.local.json`.
///
/// Scoped to the main checkout on purpose: worktree settings files are
/// regenerated per stage by the sandbox settings generator and carry a
/// superset of these registrations. An absent or unparseable file reports
/// every canonical registration as missing.
pub fn settings_local_hook_drift(repo_root: &Path) -> HookDrift {
    let settings = read_settings_local(repo_root).unwrap_or_else(|| json!({}));
    hook_drift(&settings)
}

/// Whether `.claude/settings.local.json` sets `env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`.
pub fn settings_local_has_agent_teams_env(repo_root: &Path) -> bool {
    let Some(settings) = read_settings_local(repo_root) else {
        return false;
    };
    settings
        .get("env")
        .and_then(|e| e.get("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"))
        .is_some()
}

/// Whether `.claude/settings.local.json` sets `worktree.bgIsolation` to `"none"`.
/// Without it, subagents of a main-checkout session are forced into nested git
/// worktrees that leave stray branches behind (see `ensure_loom_hooks_local`).
pub fn settings_local_has_worktree_isolation_disabled(repo_root: &Path) -> bool {
    let Some(settings) = read_settings_local(repo_root) else {
        return false;
    };
    settings.get("worktree").and_then(|w| w.get("bgIsolation")) == Some(&json!("none"))
}

/// Settings files carrying stale per-session identity env or a `LOOM_WORK_DIR`
/// pin naming a path that no longer exists.
///
/// Detector for `scrub_main_repo_settings_identity`, and defined in terms of
/// it: each candidate file is parsed, the two scrub functions are run against a
/// throwaway copy, and the path is reported iff either reported a change. That
/// keeps detector and fixer identical by construction rather than by comment.
pub fn main_repo_settings_identity_drift(repo_root: &Path) -> Vec<std::path::PathBuf> {
    let mut drifted = Vec::new();
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
        if identity_removed || stale_work_dir_removed {
            drifted.push(path);
        }
    }
    drifted
}

#[cfg(test)]
#[path = "drift/tests.rs"]
mod tests;
