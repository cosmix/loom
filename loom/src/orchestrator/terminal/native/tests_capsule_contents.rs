//! Unit tests for `session_settings/contents.rs`: the capsule document each
//! session kind launches under.

use super::contents::{capsule_settings, CapsuleInputs};
use crate::models::session::SessionType;
use crate::models::stage::{Implementer, Implementers, Stage, StageType};
use crate::sandbox::MergedSandboxConfig;
use serde_json::{json, Value};
use std::path::Path;

const ALL_KINDS: [SessionType; 5] = [
    SessionType::Stage,
    SessionType::Knowledge,
    SessionType::Merge,
    SessionType::BaseConflict,
    SessionType::Adjudication,
];

const HOOKS_DIR: &str = "/home/op/.claude/hooks/loom";

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

fn inputs<'a>(
    kind: SessionType,
    config: &'a MergedSandboxConfig,
    approved: &'a [String],
    checkout: Option<&'a Value>,
    worktree_rooted: bool,
) -> CapsuleInputs<'a> {
    CapsuleInputs {
        kind,
        sandbox: config,
        worktree_rooted,
        state_root: Path::new("/repo/.loom/work"),
        repo_root: Path::new("/repo"),
        hooks_dir: Some(Path::new(HOOKS_DIR)),
        scratch_dir: Path::new("/scratch/session-1"),
        approved,
        checkout_settings: checkout,
    }
}

fn build(
    kind: SessionType,
    config: &MergedSandboxConfig,
    approved: &[String],
    checkout: Option<&Value>,
    worktree_rooted: bool,
) -> Value {
    capsule_settings(&inputs(kind, config, approved, checkout, worktree_rooted)).unwrap()
}

/// The capsule for `kind` from where it runs, with nothing approved.
fn plain(kind: SessionType) -> Value {
    build(kind, &sandbox(false), &[], None, kind == SessionType::Stage)
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
        assert!(
            registers(&settings, "PostToolUse", "post-tool-use.sh"),
            "{kind}"
        );
        assert!(
            registers(&settings, "PreToolUse", "commit-filter.sh"),
            "{kind}"
        );
        assert!(
            !registers(&settings, "SessionStart", "session-start.sh"),
            "{kind}"
        );
        assert!(
            !registers(&settings, "PreCompact", "pre-compact.sh"),
            "{kind}"
        );
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
        "Read(//repo/.loom/work/memory/**)",
        "Edit(//repo/.loom/work/handoffs/**)",
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
fn approved_rules_are_rendered_into_permissions_allow() {
    let approved = vec!["Bash(cargo test:*)".to_string()];
    let settings = build(SessionType::Merge, &sandbox(false), &approved, None, false);
    assert!(strings(&settings, "/permissions/allow").contains(&approved[0]));
}

#[test]
fn plugin_keys_follow_the_codex_license() {
    let checkout = json!({
        "enabledPlugins": {"codex@openai-codex": true},
        "extraKnownMarketplaces": {"openai-codex": {}}
    });
    let licensed = build(
        SessionType::Stage,
        &sandbox(true),
        &[],
        Some(&checkout),
        true,
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
        true,
    );
    assert!(unlicensed.get("enabledPlugins").is_none());
    assert!(unlicensed.get("extraKnownMarketplaces").is_none());
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
            || rule.contains(".worktrees")),
        "{deny:?}"
    );
    let deny_write = strings(&from_checkout, "/sandbox/filesystem/denyWrite");
    assert!(
        !deny_write.iter().any(|path| path.contains(".worktrees")),
        "{deny_write:?}"
    );

    let from_worktree = build(SessionType::Stage, &config, &[], Some(&checkout), true);
    assert!(strings(&from_worktree, "/permissions/deny")
        .contains(&"Edit(.worktrees/other/**)".to_string()));
    assert!(strings(&from_worktree, "/sandbox/filesystem/denyWrite")
        .contains(&".worktrees/other/**".to_string()));
}

#[test]
fn a_capsule_without_a_verified_hooks_dir_registers_no_hooks() {
    let config = sandbox(false);
    let mut request = inputs(SessionType::Stage, &config, &[], None, true);
    request.hooks_dir = None;
    let settings = capsule_settings(&request).unwrap();
    assert!(settings.get("hooks").is_none(), "{settings}");
}
