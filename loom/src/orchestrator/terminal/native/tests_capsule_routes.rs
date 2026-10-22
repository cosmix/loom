//! The tool-route denies and the credential read denies of every session
//! kind's capsule: the tools that run in the Claude Code process outside the
//! Bash sandbox, and the files no session's Bash may read.

use super::*;
use crate::fs::permissions::state_root::{
    CODEX_AUTH_DENY_READ_PATH, CREDENTIAL_FILE_DENY_READ_PATHS,
};
use crate::models::session::SessionType;
use crate::models::stage::Implementer;
use crate::orchestrator::terminal::native::tests_session_settings::{
    checkout, read_capsule, sandbox_with, strings, write_capsule, ALL_KINDS,
};

const DENY_RULES: &str = "/permissions/deny";
const DENY_READ: &str = "/sandbox/filesystem/denyRead";
const DENIED_TOOLS: [&str; 5] = [
    "LSP",
    "RemoteTrigger",
    "CronCreate",
    "PushNotification",
    "Workflow",
];

type KindSets = Vec<(SessionType, std::collections::BTreeSet<String>)>;

/// Every session kind's capsule list at `pointer`, at the checkout and the
/// worktree, for a stage whose implementer lanes are `lanes`.
fn capsule_sets(pointer: &str, lanes: Vec<Implementer>) -> KindSets {
    let checkout = checkout();
    let sandbox = sandbox_with(lanes);
    let hooks = checkout.hooks_dir.clone();
    let mut sets = Vec::new();
    for kind in ALL_KINDS.into_iter().chain([SessionType::Contract]) {
        for cwd in [&checkout.repo, &checkout.worktree] {
            let id = format!("route-{kind}-{}", sets.len());
            let path = write_capsule(&checkout, &id, kind, cwd, &sandbox, Some(&hooks)).unwrap();
            sets.push((kind, strings(&read_capsule(&path), pointer)));
        }
    }
    sets
}

/// Every session kind's `/permissions/deny`.
fn all_deny_sets() -> KindSets {
    capsule_sets(DENY_RULES, vec![])
}

#[test]
fn every_kind_denies_the_tool_routes_outside_the_bash_sandbox() {
    for (kind, deny) in all_deny_sets() {
        for tool in DENIED_TOOLS {
            assert!(deny.contains(tool), "{kind} must deny {tool}: {deny:?}");
        }
    }
}

#[test]
fn the_agent_team_web_and_core_tools_stay_allowed() {
    let kept = [
        "SendMessage",
        "ListAgents",
        "WebFetch",
        "WebSearch",
        "Bash",
        "Read",
        "Edit",
        "Agent",
    ];
    for (kind, deny) in all_deny_sets() {
        for tool in kept {
            assert!(!deny.contains(tool), "{kind} must not deny {tool}");
        }
        assert!(
            !deny.iter().any(|rule| rule.starts_with("Read(")),
            "{kind} must carry no Read( deny: {deny:?}"
        );
    }
}

#[test]
fn a_licensed_stage_session_keeps_the_workflow_tool() {
    let checkout = checkout();
    let sandbox = sandbox_with(vec![]);
    let scratch_dir = checkout.scratch_root.join("route-licensed");
    let surfaces = crate::sandbox::control_surfaces::ControlSurfaces::new(
        &checkout.work_dir,
        Some(&checkout.scratch_root),
        &[],
        Some(&checkout.home),
    );
    let path = write_session_capsule(&CapsuleRequest {
        kind: SessionType::Stage,
        session_id: "route-licensed",
        sandbox: &sandbox,
        cwd: &checkout.worktree,
        work_dir: &checkout.work_dir,
        repo_root: &checkout.repo,
        hooks_dir: Some(&checkout.hooks_dir),
        scratch_dir: &scratch_dir,
        surfaces: &surfaces,
        writable_roots: &[],
        python3: None,
        python_hooks: &[],
        socket_scan_root: &checkout.tmp_root,
        workflow_licensed: true,
    })
    .unwrap();
    let deny = strings(&read_capsule(&path), DENY_RULES);
    assert!(!deny.contains("Workflow"), "{deny:?}");
    for tool in &DENIED_TOOLS[..4] {
        assert!(deny.contains(*tool), "{tool} must stay denied: {deny:?}");
    }
}

/// The bubblewrap probe checks one kind and skips without user namespaces;
/// this reads the emitted list for every kind. Membership is exact: the
/// capsule spells each path as the constant does.
#[test]
fn every_kind_denies_reading_the_credential_files_and_the_codex_auth_file() {
    let entries: Vec<&str> = CREDENTIAL_FILE_DENY_READ_PATHS
        .into_iter()
        .chain([CODEX_AUTH_DENY_READ_PATH])
        .collect();
    assert_eq!(entries.len(), 10, "{entries:?}");
    for (kind, deny_read) in capsule_sets(DENY_READ, vec![Implementer::Claude]) {
        for entry in &entries {
            assert!(
                deny_read.contains(*entry),
                "{kind} must deny reading {entry}: {deny_read:?}"
            );
        }
    }
}

/// A stage that licenses the codex lane reads and refreshes `~/.codex/auth.json`
/// in place; every other credential file stays denied.
#[test]
fn a_codex_licensed_stage_denies_the_credential_files_but_not_the_codex_auth_file() {
    let lanes = vec![Implementer::Claude, Implementer::Codex];
    for (kind, deny_read) in capsule_sets(DENY_READ, lanes) {
        for entry in CREDENTIAL_FILE_DENY_READ_PATHS {
            assert!(
                deny_read.contains(entry),
                "{kind} must deny reading {entry}: {deny_read:?}"
            );
        }
        assert!(
            !deny_read.contains(CODEX_AUTH_DENY_READ_PATH),
            "{kind} must leave the codex auth file readable: {deny_read:?}"
        );
        assert!(
            !deny_read
                .iter()
                .any(|entry| entry.ends_with(".codex/auth.json")),
            "{kind} must carry no spelling of the codex auth file: {deny_read:?}"
        );
    }
}
