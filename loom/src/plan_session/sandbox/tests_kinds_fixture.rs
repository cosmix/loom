//! Assertion-free fixture helpers and settings snapshots of the sandbox tests.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::tests_kinds::Fixture;
use crate::plan_session::kind::SessionKind;
use crate::plan_session::scope::{KickoffScope, SessionScope};
use crate::planning::runtime::RuntimePaths;

pub(super) fn kinds() -> [SessionKind; 3] {
    [
        SessionKind::Authoring,
        SessionKind::Summary,
        SessionKind::Pressure {
            plan: PathBuf::from("doc/plans/PLAN-a.md"),
        },
    ]
}

fn expected_read_denies(root: &Path, home: &Path, rt: &RuntimePaths) -> Value {
    let state = root.join(".loom/work");
    json!([
        "~/.ssh/**",
        "~/.aws/**",
        "~/.config/gcloud/**",
        "~/.gnupg/**",
        "~/.claude/.credentials.json",
        "~/.config/gh/hosts.yml",
        "~/.netrc",
        "~/.npmrc",
        "~/.git-credentials",
        "~/.docker/config.json",
        "~/.kube/config",
        "~/.pypirc",
        "~/.cargo/credentials.toml",
        "~/.cargo/credentials",
        "~/.codex/auth.json",
        rt.planning_root,
        home.join(".loom/plan"),
        format!("/{}/admin.token", state.display()),
        format!("/{}/user.token", state.display()),
        format!("/{}/completion-attestation.key", state.display()),
        state.join("terminal-history")
    ])
}

/// Compatibility helper for the Summary snapshot; other kinds use `expected_common_for`.
pub(super) fn expected_common(f: &Fixture) -> Value {
    expected_common_for(f, &SessionKind::Summary)
}

pub(super) fn expected_common_for(f: &Fixture, kind: &SessionKind) -> Value {
    let mode = match kind {
        SessionKind::Authoring | SessionKind::Pressure { .. } => "auto",
        SessionKind::Summary => "default",
    };
    json!({
        "sandbox": {
            "enabled": true, "failIfUnavailable": true,
            "autoAllowBashIfSandboxed": false, "allowUnsandboxedCommands": false,
            "network": {"strictAllowlist": true},
            "filesystem": {
                "denyWrite": [], "allowWrite": [],
                "denyRead": expected_read_denies(&f.root, &f.home, &f.runtime),
                "allowRead": [
                    f.runtime.drafts, f.runtime.claims, f.runtime.summaries_lock,
                    f.runtime.attachments
                ]
            }
        },
        "permissions": {
            "defaultMode": mode, "disableBypassPermissionsMode": "disable",
            "allow": [], "ask": [], "deny": []
        },
        "worktree": {"bgIsolation": "none"},
        "hooks": {"PreToolUse": [{
            "matcher": "Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob",
            "hooks": [{"type": "command", "command": f.command("PreToolUse"), "timeout": 10}]
        }]},
        "enabledPlugins": {"loom-ui@skills-dir": false}
    })
}

pub(super) fn expected_filesystem(root: &Path, home: &Path, rt: &RuntimePaths) -> Value {
    json!({
        "denyWrite": [
            root.join(".claude"), root.join(".git"), root.join(".loom"),
            root.join("README.md"), root.join("doc/plans/IN_PROGRESS-PLAN-b.md"),
            root.join("doc/plans/REVIEW-PLAN-b.md"), root.join("doc/plans/archive"),
            root.join("doc/plans/briefs/b"), root.join("loom"),
        ],
        "allowWrite": [rt.claims, rt.drafts, rt.summaries_lock],
        "denyRead": expected_read_denies(root, home, rt),
        "allowRead": [rt.drafts, rt.claims, rt.summaries_lock, rt.attachments],
    })
}

/// The `Edit` asks over the top-level entries both fixtures share.
pub(super) fn expected_edit_asks(root: &Path) -> [String; 6] {
    let root = root.display();
    [
        format!("Edit(/{root}/doc/loom/**)"),
        format!("Edit(/{root}/.claude/**)"),
        format!("Edit(/{root}/.git/**)"),
        format!("Edit(/{root}/.loom/**)"),
        format!("Edit(/{root}/README.md)"),
        format!("Edit(/{root}/loom/**)"),
    ]
}

pub(super) fn expected_permissions(root: &Path, rt: &RuntimePaths) -> Value {
    let asks = expected_edit_asks(root);
    let root = root.display();
    json!({
        "defaultMode": "auto",
        "disableBypassPermissionsMode": "disable",
        "allow": [
            format!("Edit(/{root}/doc/product/**)"),
            format!("Edit(/{root}/doc/plans/**)"),
            format!("Edit(/{root}/doc/notes/**)"),
            format!("Edit(/{root}/doc/index.md)"),
            format!("Edit(/{}/**)", rt.drafts.display()),
            format!("Read(/{}/**)", rt.drafts.display()),
            format!("Read(/{}/**)", rt.claims.display()),
            format!("Read(/{}/**)", rt.attachments.display()),
            "Bash(loom knowledge *)", "Bash(loom plan summary *)",
            "Bash(loom plan verify *)", "Bash(loom plan check*)", "Bash(loom map *)",
            "WebSearch",
        ],
        "ask": std::iter::once("WebFetch".to_owned()).chain(asks).collect::<Vec<_>>(),
        "deny": [
            "LSP", "RemoteTrigger", "CronCreate", "PushNotification", "Monitor", "PowerShell",
            "Workflow",
            "SendMessage", "ListAgents", "AskUserQuestion",
            format!("Edit(/{root}/doc/plans/IN_PROGRESS-PLAN-b.md)"),
            format!("Edit(/{root}/doc/plans/REVIEW-*.md)"),
            format!("Edit(/{root}/doc/plans/archive/**)"),
            format!("Edit(/{root}/doc/plans/briefs/b/**)"),
            // Archived plan c keeps a bundle deny even though its directory does not exist.
            format!("Edit(/{root}/doc/plans/briefs/c/**)"),
        ],
    })
}

pub(super) fn scope_cases(root: &Path) -> anyhow::Result<Vec<(SessionScope, Value)>> {
    let asks = |bash: &[&str]| {
        let bash = bash.iter().map(|rule| (*rule).to_owned());
        json!(bash.chain(expected_edit_asks(root)).collect::<Vec<_>>())
    };
    Ok(vec![
        (SessionScope::Open, asks(&["WebFetch"])),
        (
            SessionScope::team(vec!["doc/product/new/section_1.md".into()], Vec::new())?,
            asks(&["WebFetch", "Bash(loom knowledge *)"]),
        ),
        (
            SessionScope::Kickoff(KickoffScope {
                existing: Vec::new(),
                grant: None,
            }),
            asks(&[
                "WebFetch",
                "Bash(loom knowledge *)",
                "Bash(loom plan summary seal*)",
                "Bash(loom plan summary brief*)",
                "Bash(loom plan summary release*)",
            ]),
        ),
    ])
}
