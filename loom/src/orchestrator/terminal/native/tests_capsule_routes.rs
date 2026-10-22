//! The tool-route denies of every session kind's capsule: the tools that run
//! in the Claude Code process outside the Bash sandbox.

use super::*;
use crate::models::session::SessionType;
use crate::orchestrator::terminal::native::tests_session_settings::{
    checkout, read_capsule, sandbox_with, strings, write_capsule, ALL_KINDS,
};

const DENY_RULES: &str = "/permissions/deny";
const DENIED_TOOLS: [&str; 5] = [
    "LSP",
    "RemoteTrigger",
    "CronCreate",
    "PushNotification",
    "Workflow",
];

/// Every session kind's `/permissions/deny`, at the checkout and the worktree.
fn all_deny_sets() -> Vec<(SessionType, std::collections::BTreeSet<String>)> {
    let checkout = checkout();
    let sandbox = sandbox_with(vec![]);
    let hooks = checkout.hooks_dir.clone();
    let mut sets = Vec::new();
    for kind in ALL_KINDS.into_iter().chain([SessionType::Contract]) {
        for cwd in [&checkout.repo, &checkout.worktree] {
            let id = format!("route-{kind}-{}", sets.len());
            let path = write_capsule(&checkout, &id, kind, cwd, &sandbox, Some(&hooks)).unwrap();
            sets.push((kind, strings(&read_capsule(&path), DENY_RULES)));
        }
    }
    sets
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
