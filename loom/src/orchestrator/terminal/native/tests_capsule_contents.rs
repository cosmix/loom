//! Unit tests for `session_settings/contents.rs`: the capsule document each
//! session kind launches under.

use super::contents::{capsule_settings, CapsuleInputs};
use crate::models::session::SessionType;
use crate::models::stage::{Implementer, Implementers, Stage, StageType};
use crate::plan::schema::PermissionMode;
use crate::sandbox::control_surfaces::{session_denies, DenyInputs, SessionDenies};
use crate::sandbox::MergedSandboxConfig;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const ALL_KINDS: [SessionType; 5] = [
    SessionType::Stage,
    SessionType::Knowledge,
    SessionType::Merge,
    SessionType::BaseConflict,
    SessionType::Adjudication,
];

const HOOKS_DIR: &str = "/home/op/.claude/hooks/loom";

/// The codex lane's `~/.claude/plugins` entries, as `codex_plugin_entries` lists them.
const PLUGIN_ENTRIES: [&str; 2] = [
    ".claude/plugins/cache/**",
    ".claude/plugins/installed_plugins.json",
];

fn sandbox(codex: bool) -> MergedSandboxConfig {
    let lanes = if codex {
        vec![Implementer::Claude, Implementer::Codex]
    } else {
        vec![Implementer::Claude]
    };
    crate::sandbox::merge_config(
        &Default::default(),
        &Stage::default().sandbox,
        StageType::Standard,
        &Implementers::new(lanes),
    )
}

/// The write denies of a session in `/repo/.worktrees/s1` or the checkout.
fn denies(worktree_rooted: bool, codex: bool) -> SessionDenies {
    let entries: Vec<String> = PLUGIN_ENTRIES.iter().map(|e| e.to_string()).collect();
    session_denies(&DenyInputs {
        repo_root: Path::new("/repo"),
        state_root: Path::new("/repo/.loom/work"),
        worktree: worktree_rooted.then_some(Path::new("/repo/.worktrees/s1")),
        executable_dirs: &[PathBuf::from(HOOKS_DIR)],
        plugin_entries: codex.then_some(entries.as_slice()),
        writable_roots: &[],
    })
    .unwrap()
}

fn try_build(
    kind: SessionType,
    config: &MergedSandboxConfig,
    approved: &[String],
    checkout: Option<&Value>,
    worktree_rooted: bool,
) -> anyhow::Result<Value> {
    let denies = denies(worktree_rooted, config.implementers.includes_codex());
    capsule_settings(&CapsuleInputs {
        kind,
        sandbox: config,
        worktree_rooted,
        state_root: Path::new("/repo/.loom/work"),
        repo_root: Path::new("/repo"),
        hooks_dir: Path::new(HOOKS_DIR),
        scratch_dir: Path::new("/scratch/session-1"),
        approved,
        checkout_settings: checkout,
        denies: &denies,
    })
}

fn build(
    kind: SessionType,
    config: &MergedSandboxConfig,
    approved: &[String],
    checkout: Option<&Value>,
    worktree_rooted: bool,
) -> Value {
    try_build(kind, config, approved, checkout, worktree_rooted).unwrap()
}

/// The capsule for `kind` from where it runs, with nothing approved.
fn plain(kind: SessionType) -> Value {
    build(kind, &sandbox(false), &[], None, kind == SessionType::Stage)
}

/// Every kind's capsule from both locations, with and without the codex lane.
fn every_capsule() -> Vec<(String, Value)> {
    let mut capsules = Vec::new();
    for kind in ALL_KINDS {
        for worktree_rooted in [true, false] {
            for codex in [false, true] {
                let label = format!("{kind} worktree={worktree_rooted} codex={codex}");
                let settings = build(kind, &sandbox(codex), &[], None, worktree_rooted);
                capsules.push((label, settings));
            }
        }
    }
    capsules
}

fn strings(settings: &Value, pointer: &str) -> Vec<String> {
    settings
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Every `(event, matcher, command)` the capsule registers.
fn hooks(settings: &Value) -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    for (event, entries) in settings["hooks"].as_object().expect("a hooks block") {
        for entry in entries.as_array().expect("an event array") {
            let matcher = entry["matcher"].as_str().unwrap_or_default().to_string();
            for hook in entry["hooks"].as_array().expect("a hook list") {
                let command = hook["command"].as_str().unwrap_or_default().to_string();
                found.push((event.clone(), matcher.clone(), command));
            }
        }
    }
    found
}

fn registers(settings: &Value, event: &str, script: &str) -> bool {
    let suffix = format!("/{script}");
    hooks(settings)
        .iter()
        .any(|(registered, _, command)| registered == event && command.ends_with(&suffix))
}

#[test]
fn every_kind_gets_the_sandbox_and_the_scratch_grant_in_both_layers() {
    for kind in ALL_KINDS {
        let settings = plain(kind);
        assert_eq!(settings["sandbox"]["enabled"], json!(true), "{kind}");
        let allow_write = strings(&settings, "/sandbox/filesystem/allowWrite");
        assert!(
            allow_write.contains(&"/scratch/session-1".to_string()),
            "{kind}: {allow_write:?}"
        );
        let allow = strings(&settings, "/permissions/allow");
        assert!(
            allow.contains(&"Edit(//scratch/session-1/**)".to_string()),
            "{kind}: {allow:?}"
        );
        assert_eq!(settings["worktree"]["bgIsolation"], json!("none"), "{kind}");
        assert_eq!(
            settings["permissions"]["defaultMode"],
            json!("auto"),
            "{kind}"
        );
        assert_eq!(settings["hasTrustDialogAccepted"], json!(true), "{kind}");
        assert!(
            settings.get("env").is_none(),
            "{kind} must carry no env block: {settings}"
        );
    }
}

#[test]
fn every_kind_registers_the_relay_hook_and_runs_every_hook_under_bin_bash() {
    let relay = format!("/bin/bash {HOOKS_DIR}/loom-relay.sh");
    for kind in ALL_KINDS {
        let registered = hooks(&plain(kind));
        assert!(
            registered.iter().any(|(event, matcher, command)| {
                event == "PostToolUse" && matcher == "Bash" && *command == relay
            }),
            "{kind}: {registered:?}"
        );
        for (_, _, command) in &registered {
            assert!(command.starts_with("/bin/bash /"), "{kind}: {command}");
        }
    }
}

#[test]
fn stage_and_knowledge_get_every_session_hook_and_the_completion_broker() {
    for kind in [SessionType::Stage, SessionType::Knowledge] {
        let settings = plain(kind);
        for (event, script) in [
            ("SessionStart", "session-start.sh"),
            ("PostToolUse", "post-tool-use.sh"),
            ("PreCompact", "pre-compact.sh"),
            ("SessionEnd", "session-end.sh"),
            ("Stop", "learning-validator.sh"),
            ("SubagentStart", "subagent-start.sh"),
            ("SubagentStop", "subagent-stop.sh"),
            ("PreToolUse", "loom-control-complete.sh"),
            ("PostToolUse", "loom-control-complete.sh"),
            ("PreToolUse", "commit-filter.sh"),
        ] {
            assert!(
                registers(&settings, event, script),
                "{kind} must register {script} on {event}"
            );
        }
    }
}

#[test]
fn merge_base_conflict_and_adjudication_get_only_the_heartbeat_and_the_guards() {
    for kind in [
        SessionType::Merge,
        SessionType::BaseConflict,
        SessionType::Adjudication,
    ] {
        let settings = plain(kind);
        for (present, event, script) in [
            (true, "PostToolUse", "post-tool-use.sh"),
            (true, "PreToolUse", "commit-filter.sh"),
            (false, "SessionStart", "session-start.sh"),
            (false, "PreCompact", "pre-compact.sh"),
        ] {
            let registered = registers(&settings, event, script);
            assert_eq!(registered, present, "{kind} {script}");
        }
        assert!(
            !hooks(&settings)
                .iter()
                .any(|(_, _, command)| command.ends_with("/loom-control-complete.sh")),
            "{kind} must not register the completion broker"
        );
    }
}

#[test]
fn the_state_root_grants_are_resolved_and_the_tokens_denied() {
    let settings = plain(SessionType::Stage);
    let allow = strings(&settings, "/permissions/allow");
    for rule in [
        "Read(//repo/.loom/work/config.toml)",
        "Read(//repo/.loom/work/signals/**)",
        "Read(//repo/.loom/work/handoffs/**)",
        "Read(//repo/.loom/work/memory/**)",
        "Read(//repo/doc/plans/**)",
    ] {
        assert!(
            allow.contains(&rule.to_string()),
            "missing {rule}: {allow:?}"
        );
    }
    let deny_read = strings(&settings, "/sandbox/filesystem/denyRead");
    assert!(
        deny_read.contains(&"//repo/.loom/work/admin.token".to_string()),
        "{deny_read:?}"
    );
}

#[test]
fn no_capsule_grants_a_handoff_edit_in_any_spelling() {
    for (label, settings) in every_capsule() {
        let allow = strings(&settings, "/permissions/allow");
        assert!(
            !allow
                .iter()
                .any(|rule| rule.starts_with("Edit(") && rule.contains("handoffs")),
            "{label}: {allow:?}"
        );
    }
}

/// Plan section 10's shape: a literal path, or a literal directory followed
/// by `/**`; no other glob character and no `..` anywhere.
fn literal_shape(path: &str) -> bool {
    let literal = path.strip_suffix("/**").unwrap_or(path);
    !literal.is_empty()
        && !literal.contains(['*', '?', '[', '{'])
        && !literal.split('/').any(|part| part == "..")
}

#[test]
fn every_generated_path_rule_and_sandbox_path_has_the_literal_shape() {
    for (label, settings) in every_capsule() {
        for pointer in ["/permissions/allow", "/permissions/deny"] {
            for rule in strings(&settings, pointer) {
                let Some((tool, rest)) = rule.split_once('(') else {
                    continue;
                };
                if !["Edit", "Read", "Write", "MultiEdit"].contains(&tool) {
                    continue;
                }
                let path = rest.strip_suffix(')').expect("a closed rule");
                let spelled = !path.starts_with('/') || path.starts_with("//");
                assert!(spelled && literal_shape(path), "{label}: {rule}");
                let read_deny = tool == "Read" && pointer.ends_with("deny");
                assert!(!read_deny, "{label}: a Read deny {rule}");
            }
        }
        for list in ["allowWrite", "denyWrite", "denyRead"] {
            for path in strings(&settings, &format!("/sandbox/filesystem/{list}")) {
                // The write denies are exact literals, never a pattern.
                let pattern = list == "denyWrite" && path.contains('*');
                assert!(literal_shape(&path) && !pattern, "{label}: {list} {path}");
            }
        }
    }
}

#[test]
fn a_config_validate_config_refuses_is_refused_for_every_kind() {
    let mut config = sandbox(false);
    config.permission_mode = PermissionMode::BypassPermissions;
    for kind in ALL_KINDS {
        for worktree_rooted in [true, false] {
            let error = try_build(kind, &config, &[], None, worktree_rooted).unwrap_err();
            assert!(
                format!("{error:#}").contains("bypass-permissions"),
                "{kind}: {error:#}"
            );
        }
    }
}

#[test]
fn plugin_keys_follow_the_codex_license() {
    let checkout = json!({
        "enabledPlugins": {"codex@openai-codex": true},
        "extraKnownMarketplaces": {"openai-codex": {}}
    });
    for worktree_rooted in [true, false] {
        let licensed = build(
            SessionType::Stage,
            &sandbox(true),
            &[],
            Some(&checkout),
            worktree_rooted,
        );
        assert_eq!(licensed["enabledPlugins"], checkout["enabledPlugins"]);
        assert_eq!(
            licensed["extraKnownMarketplaces"],
            checkout["extraKnownMarketplaces"]
        );

        let unlicensed = build(
            SessionType::Stage,
            &sandbox(false),
            &[],
            Some(&checkout),
            worktree_rooted,
        );
        assert!(unlicensed.get("enabledPlugins").is_none());
        assert!(unlicensed.get("extraKnownMarketplaces").is_none());
    }
}

#[test]
fn the_checkouts_denies_are_carried_and_escape_rules_follow_the_location() {
    let checkout = json!({"permissions": {"deny": [
        "Bash(rm -rf:*)",
        "Read(//secret/**)",
        "Edit(doc/loom/knowledge/**)",
        "Edit(.worktrees/other/**)"
    ]}});
    let mut config = sandbox(false);
    config
        .filesystem
        .deny_write
        .push(".worktrees/other/**".to_string());

    let from_checkout = build(SessionType::Merge, &config, &[], Some(&checkout), false);
    let deny = strings(&from_checkout, "/permissions/deny");
    assert!(deny.contains(&"Bash(rm -rf:*)".to_string()), "{deny:?}");
    assert!(
        !deny.iter().any(|rule| rule.starts_with("Read(")
            || rule.contains("doc/loom/knowledge")
            || rule == "Edit(.worktrees/other/**)"),
        "{deny:?}"
    );
    let deny_write = strings(&from_checkout, "/sandbox/filesystem/denyWrite");
    assert!(
        !deny_write.contains(&".worktrees/other/**".to_string()),
        "{deny_write:?}"
    );

    let from_worktree = build(SessionType::Stage, &config, &[], Some(&checkout), true);
    assert!(strings(&from_worktree, "/permissions/deny")
        .contains(&"Edit(.worktrees/other/**)".to_string()));
    assert!(strings(&from_worktree, "/sandbox/filesystem/denyWrite")
        .contains(&".worktrees/other/**".to_string()));
}
