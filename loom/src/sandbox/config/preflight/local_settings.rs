//! The keys loom used to write into the operator's
//! `R/.claude/settings.local.json` (plan section 12). Every session's capsule
//! carries them now, so `loom repair --fix` strips them and `loom run` warns
//! while any remain, both through this one definition. The global guard
//! hooks, `env.CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`, `worktree.bgIsolation`
//! and `permissions.defaultMode` stay.

use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

use crate::hooks::HookEvent;
use crate::sandbox::grant_paths::edit_rule;

/// What a settings document carries of the loom-written keys.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LoomWrittenKeys {
    /// The `sandbox` block.
    pub sandbox: bool,
    /// `permissions.allow` rules: state-directory reads and handoff edits in
    /// every spelling, the plans read, and the `Edit` rules derived from the
    /// removed `sandbox.filesystem.allowWrite`.
    pub allow_rules: Vec<String>,
    /// The session `HookEvent` registrations, as `Event:script`.
    pub session_hooks: Vec<String>,
    /// `env.LOOM_WORK_DIR`.
    pub work_dir_env: bool,
}

impl LoomWrittenKeys {
    pub(crate) fn is_empty(&self) -> bool {
        !self.sandbox
            && self.allow_rules.is_empty()
            && self.session_hooks.is_empty()
            && !self.work_dir_env
    }

    /// What is present, in one line: `sandbox block, 3 permission rule(s)`.
    pub(crate) fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.sandbox {
            parts.push("sandbox block".to_string());
        }
        if !self.allow_rules.is_empty() {
            parts.push(format!("{} permission rule(s)", self.allow_rules.len()));
        }
        if !self.session_hooks.is_empty() {
            let count = self.session_hooks.len();
            parts.push(format!("{count} session hook registration(s)"));
        }
        if self.work_dir_env {
            parts.push("env.LOOM_WORK_DIR".to_string());
        }
        parts.join(", ")
    }
}

/// The loom-written keys `R/.claude/settings.local.json` carries; empty when
/// the file is absent or not JSON.
pub(crate) fn settings_local_loom_keys(repo_root: &Path) -> LoomWrittenKeys {
    match read_settings(&settings_local_path(repo_root)) {
        Some(mut settings) => strip_loom_written_keys(&mut settings, repo_root),
        None => LoomWrittenKeys::default(),
    }
}

/// Strip the loom-written keys from `R/.claude/settings.local.json` in place,
/// leaving everything else as it was. An absent file, or one with nothing to
/// strip, is not written.
pub(crate) fn strip_settings_local_loom_keys(repo_root: &Path) -> Result<()> {
    let path = settings_local_path(repo_root);
    let Some(mut settings) = read_settings(&path) else {
        return Ok(());
    };
    if strip_loom_written_keys(&mut settings, repo_root).is_empty() {
        return Ok(());
    }
    let text = serde_json::to_string_pretty(&settings)
        .with_context(|| format!("Failed to serialize {}", path.display()))?;
    crate::fs::locking::locked_write(&path, &text)
        .with_context(|| format!("Failed to write {}", path.display()))
}

/// Remove every loom-written key from `settings`, reporting what was there.
pub(super) fn strip_loom_written_keys(settings: &mut Value, repo_root: &Path) -> LoomWrittenKeys {
    let Some(settings) = settings.as_object_mut() else {
        return LoomWrittenKeys::default();
    };
    // Read before the sandbox block that holds them goes.
    let edit_rules = allow_write_edit_rules(settings);
    LoomWrittenKeys {
        sandbox: settings.remove("sandbox").is_some(),
        allow_rules: strip_allow_rules(settings, &edit_rules, &plan_read_rules(repo_root)),
        session_hooks: strip_session_hooks(settings),
        work_dir_env: strip_work_dir_env(settings),
    }
}

fn settings_local_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".claude").join("settings.local.json")
}

fn read_settings(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The `Edit(...)` rules derived from each entry of the document's
/// `sandbox.filesystem.allowWrite`, as filtered by the capsule built through
/// `sandbox::settings::build_settings`.
fn allow_write_edit_rules(settings: &Map<String, Value>) -> Vec<String> {
    settings
        .get("sandbox")
        .and_then(|sandbox| sandbox.pointer("/filesystem/allowWrite"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty() && !path.contains("../"))
        .map(edit_rule)
        .collect()
}

/// `Read(//R/doc/plans/**)`, for the repository as given and canonicalized.
fn plan_read_rules(repo_root: &Path) -> Vec<String> {
    let mut roots = vec![repo_root.to_path_buf()];
    roots.extend(repo_root.canonicalize().ok());
    roots
        .iter()
        .map(|root| format!("Read(/{}/**)", root.join("doc").join("plans").display()))
        .collect()
}

fn strip_allow_rules(
    settings: &mut Map<String, Value>,
    edit_rules: &[String],
    plan_reads: &[String],
) -> Vec<String> {
    let allow = settings
        .get_mut("permissions")
        .and_then(|permissions| permissions.get_mut("allow"))
        .and_then(Value::as_array_mut);
    let Some(allow) = allow else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    allow.retain(|rule| {
        let Some(rule) = rule.as_str() else {
            return true;
        };
        let owned = |rules: &[String]| rules.iter().any(|known| known == rule);
        let loom_written = is_state_rule(rule) || owned(edit_rules) || owned(plan_reads);
        if loom_written {
            removed.push(rule.to_string());
        }
        !loom_written
    });
    removed
}

/// A state-directory read rule, or a handoff edit rule, in any spelling:
/// relative, `../`-prefixed, resolved-absolute or parent-glob, under either
/// state-root layout.
fn is_state_rule(rule: &str) -> bool {
    let Some((tool, path)) = rule.strip_suffix(')').and_then(|rule| rule.split_once('(')) else {
        return false;
    };
    let Some(inside) = state_subpath(path) else {
        return false;
    };
    match tool {
        "Read" => true,
        "Edit" | "Write" => inside == "handoffs" || inside.starts_with("handoffs/"),
        _ => false,
    }
}

/// The part of `path` inside its state root (`.loom/work`, or the legacy
/// `.work`), `""` for the root itself; `None` when `path` names neither.
fn state_subpath(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').collect();
    segments.iter().enumerate().find_map(|(index, segment)| {
        let width = match *segment {
            ".work" => 1,
            ".loom" if segments.get(index + 1) == Some(&"work") => 2,
            _ => return None,
        };
        Some(segments[index + width..].join("/"))
    })
}

/// Remove the session `HookEvent` registrations: `hooks.<Event>` entries that
/// run only that event's loom script. An event array they empty goes too.
fn strip_session_hooks(settings: &mut Map<String, Value>) -> Vec<String> {
    let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for event in HookEvent::all() {
        let (name, script) = (event.to_string(), event.script_name());
        let Some(entries) = hooks.get_mut(&name).and_then(Value::as_array_mut) else {
            continue;
        };
        let (session, kept): (Vec<Value>, Vec<Value>) = std::mem::take(entries)
            .into_iter()
            .partition(|entry| runs_only(entry, script));
        *entries = kept;
        removed.extend(session.iter().map(|_| format!("{name}:{script}")));
        if !session.is_empty() && entries.is_empty() {
            hooks.remove(&name);
        }
    }
    removed
}

/// Whether every command of a hook entry runs `script`.
fn runs_only(entry: &Value, script: &str) -> bool {
    let commands: Vec<&str> = entry
        .get("hooks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hook| hook.get("command").and_then(Value::as_str))
        .collect();
    !commands.is_empty()
        && commands.iter().all(|command| {
            let path = command.split_whitespace().last().unwrap_or_default();
            Path::new(path)
                .file_name()
                .is_some_and(|name| name == script)
        })
}

/// Remove `env.LOOM_WORK_DIR`, and `env` itself if that empties it.
fn strip_work_dir_env(settings: &mut Map<String, Value>) -> bool {
    let Some(env) = settings.get_mut("env").and_then(Value::as_object_mut) else {
        return false;
    };
    let removed = env.remove("LOOM_WORK_DIR").is_some();
    if removed && env.is_empty() {
        settings.remove("env");
    }
    removed
}
