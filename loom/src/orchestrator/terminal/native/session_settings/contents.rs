//! The capsule's settings document, built from already-resolved inputs.
//!
//! Pure: nothing here reads the environment or the filesystem. In phase 1 of
//! `doc/plans/PLAN-loom-state-confinement.md` the document carries the grants
//! each session kind already had from its `.claude/settings.local.json`, plus
//! the session's scratch grant, the approved-permissions list and the relay
//! hook.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::Path;

use crate::hooks::HookEvent;
use crate::models::session::SessionType;
use crate::sandbox::{
    carry_forward_denies, strip_worktree_escape_denies, MergedSandboxConfig, STATE_READ_DIRS,
};

/// The relay hook every kind registers (PostToolUse, matcher `Bash`).
const RELAY_SCRIPT: &str = "loom-relay.sh";
/// The completion broker's hook; only the kinds that complete through it keep it.
const CONTROL_COMPLETE_SCRIPT: &str = "loom-control-complete.sh";
/// Top-level keys copied from the checkout's local settings when the codex
/// lane is licensed.
const PLUGIN_KEYS: [&str; 2] = ["enabledPlugins", "extraKnownMarketplaces"];

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
    pub hooks_dir: Option<&'a Path>,
    /// `<scratch_root>/<session-id>`, absolute.
    pub scratch_dir: &'a Path,
    /// Approved rules, already filtered for control surfaces.
    pub approved: &'a [String],
    /// The checkout's `.claude/settings.local.json`, when it has one.
    pub checkout_settings: Option<&'a Value>,
}

/// The capsule document: the sandbox block and permission rules
/// `sandbox::generate_settings_json` builds from the stage's config
/// (`defaultMode` and `worktree.bgIsolation` included), the resolved
/// state-root grants, the scratch grant, the approved list, the checkout's
/// carried deny rules, the kind's hooks, and (codex lane) the plugin keys. It
/// never carries an `env` block: `LOOM_WORK_DIR` and every identity variable
/// come from the wrapper alone.
pub(super) fn capsule_settings(inputs: &CapsuleInputs<'_>) -> Result<Value> {
    let config = located_config(inputs.sandbox, inputs.worktree_rooted);
    crate::sandbox::validate_emittable(&config)?;
    let mut settings = crate::sandbox::generate_settings_json(&config);
    add_state_root_grants(&mut settings, inputs.state_root, inputs.repo_root)?;
    add_scratch_grant(&mut settings, inputs.scratch_dir)?;
    extend_strings(&mut settings, &["permissions", "allow"], inputs.approved);
    let denies = carried_denies(inputs.checkout_settings, inputs.worktree_rooted);
    extend_strings(&mut settings, &["permissions", "deny"], &denies);
    if let Some(hooks_dir) = inputs.hooks_dir {
        settings["hooks"] = capsule_hooks(inputs.kind, hooks_dir);
    }
    if inputs.sandbox.implementers.includes_codex() {
        copy_plugin_keys(&mut settings, inputs.checkout_settings);
    }
    settings["hasTrustDialogAccepted"] = json!(true);
    Ok(settings)
}

/// The stage's config as the session's location sees it. From the checkout,
/// worktree-relative escape rules (`../../**`, `.worktrees`) would resolve
/// against the operator's home directory, so they are dropped, as
/// `sandbox::write_settings` drops them for a main-repository target.
fn located_config(sandbox: &MergedSandboxConfig, worktree_rooted: bool) -> MergedSandboxConfig {
    let mut config = sandbox.clone();
    if !worktree_rooted {
        strip_worktree_escape_denies(&mut config);
    }
    config
}

/// The absolute spellings of the narrow state-root grants (Claude Code
/// resolves the `.loom/work` symlink before matching, so a worktree session
/// needs them; `generate_settings_json` emits the relative ones), the handoff
/// write grant sessions keep in phase 1, the plans read, and the token-file
/// read denies.
fn add_state_root_grants(settings: &mut Value, state_root: &Path, repo_root: &Path) -> Result<()> {
    let root = utf8(state_root)?;
    let plans = repo_root.join("doc").join("plans");
    let mut allow = vec![format!("Read(/{root}/config.toml)")];
    allow.extend(
        STATE_READ_DIRS
            .iter()
            .map(|dir| format!("Read(/{root}/{dir}/**)")),
    );
    allow.push(format!("Edit(/{root}/handoffs/**)"));
    allow.push(format!("Read(/{}/**)", utf8(&plans)?));
    extend_strings(settings, &["permissions", "allow"], &allow);
    let token_denies = crate::fs::permissions::state_root::token_deny_paths(root);
    extend_strings(
        settings,
        &["sandbox", "filesystem", "denyRead"],
        &token_denies,
    );
    Ok(())
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

/// The checkout's own `permissions.deny` rules, carried the way
/// `sandbox::write_settings` carries a target's existing denies: never a
/// `Read(` deny, never a knowledge-directory deny, no escape rules from the
/// checkout, and inert `Write(` rules migrated to `Edit(`.
fn carried_denies(checkout_settings: Option<&Value>, worktree_rooted: bool) -> Vec<String> {
    let Some(deny) = checkout_settings
        .and_then(|settings| settings.pointer("/permissions/deny"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let existing: Vec<String> = deny
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    carry_forward_denies(existing, worktree_rooted)
}

fn copy_plugin_keys(settings: &mut Value, checkout_settings: Option<&Value>) {
    let (Some(source), Some(target)) = (
        checkout_settings.and_then(Value::as_object),
        settings.as_object_mut(),
    ) else {
        return;
    };
    for key in PLUGIN_KEYS {
        if let Some(value) = source.get(key) {
            target.entry(key).or_insert_with(|| value.clone());
        }
    }
}

/// The kind's `hooks` block: loom's global guard set for every kind (the
/// completion broker's hook only for Stage and Knowledge), every session hook
/// event for Stage and Knowledge but only the PostToolUse heartbeat for the
/// others, and the relay hook for every kind.
fn capsule_hooks(kind: SessionType, hooks_dir: &Path) -> Value {
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
    in_bash_form(&mut hooks);
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

/// Rewrite every hook command `<script>` as `/bin/bash <script>`, so no hook
/// depends on a script's execute bit or its shebang's interpreter lookup.
fn in_bash_form(hooks: &mut Value) {
    let Some(events) = hooks.as_object_mut() else {
        return;
    };
    for entries in events.values_mut().filter_map(Value::as_array_mut) {
        let commands = entries
            .iter_mut()
            .filter_map(|entry| entry.get_mut("hooks"))
            .filter_map(Value::as_array_mut)
            .flatten();
        for hook in commands {
            let Some(command) = hook["command"].as_str().map(str::to_owned) else {
                continue;
            };
            hook["command"] = json!(format!("/bin/bash {}", escape(Cow::Owned(command))));
        }
    }
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
