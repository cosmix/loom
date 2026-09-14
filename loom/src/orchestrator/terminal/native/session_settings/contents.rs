//! The capsule's settings document, built from already-resolved inputs.
//!
//! Pure: nothing here reads the environment or the filesystem. The document
//! is `sandbox::build_settings`'s for the session's location, plus the plans
//! read, the session's scratch grant, the approved-permissions list, the
//! location and control-surface write denies
//! (`doc/plans/PLAN-loom-state-confinement.md`, sections 10 and 11) and the
//! kind's hooks.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::hooks::HookEvent;
use crate::models::session::SessionType;
use crate::sandbox::control_surfaces::SessionDenies;
use crate::sandbox::{MergedSandboxConfig, SettingsTarget};

/// The relay hook every kind registers (PostToolUse, matcher `Bash`).
const RELAY_SCRIPT: &str = "loom-relay.sh";
/// The completion broker's hook; only the kinds that complete through it keep it.
const CONTROL_COMPLETE_SCRIPT: &str = "loom-control-complete.sh";

/// Everything [`capsule_settings`] reads.
pub(super) struct CapsuleInputs<'a> {
    pub kind: SessionType,
    /// The stage's merged, path-expanded sandbox config.
    pub sandbox: &'a MergedSandboxConfig,
    /// Whether the session runs in a stage worktree rather than the checkout.
    pub worktree_rooted: bool,
    /// The canonical state root (`.loom/work`).
    pub state_root: &'a Path,
    pub repo_root: &'a Path,
    /// The verified loom hooks directory every registration runs from.
    pub hooks_dir: &'a Path,
    /// `<scratch_root>/<session-id>`, absolute.
    pub scratch_dir: &'a Path,
    /// Approved rules, already filtered for control surfaces.
    pub approved: &'a [String],
    /// The checkout's `.claude/settings.local.json`, when it has one.
    pub checkout_settings: Option<&'a Value>,
    /// The location and control-surface write denies, both layers.
    pub denies: &'a SessionDenies,
    /// The first `python3` on the pinned hook PATH, the interpreter written
    /// for a Python hook command; `None` drops any such hook instead.
    pub python3: Option<&'a Path>,
    /// Every regular file directly in the hooks directory whose first line
    /// is a python shebang.
    pub python_hooks: &'a [PathBuf],
}

/// The capsule document: what `sandbox::build_settings` builds from the
/// stage's config for the session's location (`defaultMode` and
/// `worktree.bgIsolation` included, the checkout's deny rules carried, and
/// with the codex lane its plugin keys), then the plans read, the scratch
/// grant, the approved list, the session's write denies and the kind's hooks.
/// The config must pass `sandbox::validate_config` first. It never carries
/// an `env` block: `LOOM_WORK_DIR` and every identity variable come from the
/// wrapper alone.
pub(super) fn capsule_settings(inputs: &CapsuleInputs<'_>) -> Result<Value> {
    crate::sandbox::validate_config(inputs.sandbox)?;
    let no_checkout_settings = json!({});
    let mut settings = crate::sandbox::build_settings(
        inputs.sandbox,
        &SettingsTarget {
            is_worktree: inputs.worktree_rooted,
            state_root: Some(utf8(inputs.state_root)?),
            existing: inputs.checkout_settings.unwrap_or(&no_checkout_settings),
            carry_plugin_keys: inputs.sandbox.implementers.includes_codex(),
        },
    )?;
    let plans = inputs.repo_root.join("doc").join("plans");
    let plans_read = format!("Read(/{}/**)", utf8(&plans)?);
    extend_strings(&mut settings, &["permissions", "allow"], &[plans_read]);
    add_scratch_grant(&mut settings, inputs.scratch_dir)?;
    extend_strings(&mut settings, &["permissions", "allow"], inputs.approved);
    let denies = inputs.denies;
    let deny_write = ["sandbox", "filesystem", "denyWrite"];
    extend_strings(&mut settings, &deny_write, &denies.deny_write);
    extend_strings(&mut settings, &["permissions", "deny"], &denies.edit);
    settings["hooks"] = capsule_hooks(
        inputs.kind,
        inputs.hooks_dir,
        inputs.python3,
        inputs.python_hooks,
    );
    settings["hasTrustDialogAccepted"] = json!(true);
    Ok(settings)
}

/// The session's own scratch directory, writable in both layers: the OS
/// sandbox takes the plain absolute path, the permission rule the `//`
/// absolute spelling (a single leading `/` is project-relative there).
fn add_scratch_grant(settings: &mut Value, scratch_dir: &Path) -> Result<()> {
    let dir = utf8(scratch_dir)?;
    extend_strings(
        settings,
        &["sandbox", "filesystem", "allowWrite"],
        &[dir.to_string()],
    );
    extend_strings(
        settings,
        &["permissions", "allow"],
        &[format!("Edit(/{dir}/**)")],
    );
    Ok(())
}

/// The kind's `hooks` block: loom's global guard set for every kind (the
/// completion broker's hook only for Stage and Knowledge), every session hook
/// event for Stage and Knowledge but only the PostToolUse heartbeat for the
/// others, and the relay hook for every kind.
fn capsule_hooks(
    kind: SessionType,
    hooks_dir: &Path,
    python3: Option<&Path>,
    python_hooks: &[PathBuf],
) -> Value {
    let brokered = matches!(kind, SessionType::Stage | SessionType::Knowledge);
    let mut hooks = crate::fs::permissions::guard_hooks_config(&hooks_dir.display().to_string());
    if !brokered {
        drop_script(&mut hooks, CONTROL_COMPLETE_SCRIPT);
    }
    let events: &[HookEvent] = if brokered {
        HookEvent::all()
    } else {
        &[HookEvent::PostToolUse]
    };
    for event in events {
        let script = hooks_dir.join(event.script_name());
        push_rule(&mut hooks, &event.to_string(), "*", &script);
    }
    let relay = hooks_dir.join(RELAY_SCRIPT);
    push_rule(
        &mut hooks,
        &HookEvent::PostToolUse.to_string(),
        "Bash",
        &relay,
    );
    with_interpreters(&mut hooks, python3, python_hooks);
    hooks
}

/// Remove every registration whose command runs `script`.
fn drop_script(hooks: &mut Value, script: &str) {
    let suffix = format!("/{script}");
    let Some(events) = hooks.as_object_mut() else {
        return;
    };
    for entries in events.values_mut().filter_map(Value::as_array_mut) {
        entries.retain(|entry| {
            !entry["hooks"].as_array().is_some_and(|list| {
                list.iter().any(|hook| {
                    hook["command"]
                        .as_str()
                        .is_some_and(|c| c.ends_with(&suffix))
                })
            })
        });
    }
}

fn push_rule(hooks: &mut Value, event: &str, matcher: &str, script: &Path) {
    let rule = json!({
        "matcher": matcher,
        "hooks": [{"type": "command", "command": script.display().to_string()}],
    });
    if let Some(entries) = entry(hooks, event, json!([])).as_array_mut() {
        entries.push(rule);
    }
}

/// Rewrite every hook command `<script>` with the interpreter that must run
/// it, so no hook depends on a script's execute bit or its shebang's
/// interpreter lookup: `<python3> <script>` when `script` is one of
/// `python_hooks` and `python3` is `Some`, `/bin/bash <script>` for every
/// other hook. A python hook with no `python3` on the pinned hook PATH is
/// dropped instead of being run under the wrong interpreter, along with its
/// matcher entry once that entry's `hooks` array is left empty.
fn with_interpreters(hooks: &mut Value, python3: Option<&Path>, python_hooks: &[PathBuf]) {
    let Some(events) = hooks.as_object_mut() else {
        return;
    };
    for entries in events.values_mut().filter_map(Value::as_array_mut) {
        for entry in entries.iter_mut() {
            let Some(commands) = entry.get_mut("hooks").and_then(Value::as_array_mut) else {
                continue;
            };
            commands.retain_mut(|hook| rewrite_command(hook, python3, python_hooks));
        }
        entries.retain(|entry| {
            entry
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|list| !list.is_empty())
        });
    }
}

/// Rewrite one hook's `command` in place; `false` means drop it (an
/// unrecognized-interpreter python hook with no `python3` available).
fn rewrite_command(hook: &mut Value, python3: Option<&Path>, python_hooks: &[PathBuf]) -> bool {
    let Some(command) = hook["command"].as_str().map(str::to_owned) else {
        return true;
    };
    if python_hooks
        .iter()
        .any(|script| script == Path::new(&command))
    {
        return match python3 {
            Some(interpreter) => {
                hook["command"] = json!(format!(
                    "{} {}",
                    escape(Cow::Owned(interpreter.display().to_string())),
                    escape(Cow::Owned(command)),
                ));
                true
            }
            None => {
                tracing::warn!(
                    "dropping Python hook {command}: no python3 on the pinned hook PATH"
                );
                false
            }
        };
    }
    hook["command"] = json!(format!("/bin/bash {}", escape(Cow::Owned(command))));
    true
}

/// Append each of `items` not already present to the string array at `path`,
/// creating the objects and the array on the way.
fn extend_strings(settings: &mut Value, path: &[&str], items: &[String]) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let mut node = settings;
    for key in parents {
        node = entry(node, key, json!({}));
    }
    let Some(array) = entry(node, last, json!([])).as_array_mut() else {
        return;
    };
    for item in items {
        if !array
            .iter()
            .any(|existing| existing.as_str() == Some(item.as_str()))
        {
            array.push(json!(item));
        }
    }
}

/// `node[key]`, inserting `default` when missing; `node` becomes an object
/// first if it is not one.
fn entry<'v>(node: &'v mut Value, key: &str, default: Value) -> &'v mut Value {
    if !node.is_object() {
        *node = json!({});
    }
    node.as_object_mut()
        .expect("just made an object")
        .entry(key)
        .or_insert(default)
}

fn utf8(path: &Path) -> Result<&str> {
    path.to_str()
        .with_context(|| format!("{} is not valid UTF-8", path.display()))
}
