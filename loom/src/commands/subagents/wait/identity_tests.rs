use std::ffi::OsString;
use std::fs;

use super::*;
use crate::commands::subagents::wait::parent_scan::candidates_from_entries;

const FIRST: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const SECOND: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";

fn fixture() -> (tempfile::TempDir, IdentityScope, StartedAgentTypeIndex) {
    let dir = tempfile::tempdir().unwrap();
    let stage = dir.path().join("work/subagents/stage-a");
    fs::create_dir_all(&stage).unwrap();
    let rows = [FIRST, SECOND].map(|parent| {
        format!(r#"{{"agent_id":"agent-a","agent_type":"review","parent_session_id":"{parent}","stage_id":"stage-a","loom_session_id":"loom-a"}}"#)
    });
    fs::write(stage.join("starts.jsonl"), rows.join("\n") + "\n").unwrap();
    let scope = IdentityScope {
        work_dir: dir.path().join("work"),
        canonical_repo: dir.path().to_owned(),
        canonical_worktree: dir.path().to_owned(),
        projects_root: dir.path().join("projects"),
        stage_id: "stage-a".into(),
        loom_session_id: "loom-a".into(),
    };
    let project = scope
        .projects_root
        .join(project_slug(&scope.canonical_worktree));
    for parent in [FIRST, SECOND] {
        fs::create_dir_all(project.join(parent)).unwrap();
    }
    let starts = StartedAgentTypeIndex::load(Some(&scope.work_dir));
    (dir, scope, starts)
}

fn claude() -> WorkerSpec {
    "claude:agent-a".parse().unwrap()
}

fn authorization(scope: &IdentityScope, parent: &str) -> CodexAuthorization {
    CodexAuthorization {
        stage_id: scope.stage_id.clone(),
        loom_session_id: scope.loom_session_id.clone(),
        parent_session_id: parent.into(),
        forwarder_agent_id: "agent-a".into(),
        unit_id: "unit-a".into(),
        invocation_id: "invocation-a".into(),
        model: "model".into(),
        effort: "high".into(),
        authorized_at: chrono::Utc::now(),
        selected_companion: scope.canonical_repo.clone(),
        companion_version: "1".into(),
        effective_state_root: scope.canonical_repo.clone(),
        workspace_root: scope.canonical_worktree.clone(),
        tool_use_id: "tool-a".into(),
    }
}

#[test]
fn overflow_never_returns_a_partial_unique_candidate() {
    let (_dir, scope, starts) = fixture();
    let bounded = std::iter::once(Ok((OsString::from(FIRST), true)))
        .chain((0..511).map(|index| Ok((OsString::from(format!("file-{index}")), false))));
    assert_eq!(
        candidates_from_entries(&scope, "agent-a", &starts, bounded).unwrap(),
        BTreeSet::from([FIRST.to_owned()])
    );
    let first = std::iter::once(Ok((OsString::from(FIRST), true)));
    let unrelated = (0..511).map(|index| Ok((OsString::from(format!("file-{index}")), false)));
    let hidden = std::iter::once(Ok((OsString::from(SECOND), true)));
    let error = candidates_from_entries(
        &scope,
        "agent-a",
        &starts,
        first.chain(unrelated).chain(hidden),
    )
    .unwrap_err();
    assert!(error.to_string().contains("scan exceeds 512 entries"));
    let unrelated = (0..512).map(|index| Ok((OsString::from(format!("file-{index}")), false)));
    let hidden = std::iter::once(Ok((OsString::from(FIRST), true)));
    assert!(candidates_from_entries(&scope, "agent-a", &starts, unrelated.chain(hidden)).is_err());
}

#[test]
fn overflowing_history_fails_implicit_but_allows_explicit_parent() {
    let (_dir, scope, starts) = fixture();
    let project = scope
        .projects_root
        .join(project_slug(&scope.canonical_worktree));
    for index in 0..511 {
        fs::write(project.join(format!("unrelated-{index}")), "").unwrap();
    }
    let specs = [claude()];
    assert!(common_parent(&scope, &specs, None, &starts, &[])
        .unwrap_err()
        .to_string()
        .contains("scan exceeds 512 entries"));
    assert_eq!(
        common_parent(&scope, &specs, Some(FIRST), &starts, &[]).unwrap(),
        FIRST
    );
}

#[test]
fn mixed_workers_intersect_parents_and_explicit_selection_still_checks_ownership() {
    let (_dir, scope, starts) = fixture();
    let specs = [claude(), "codex:unit-a".parse().unwrap()];
    let auth = [authorization(&scope, SECOND)];
    assert_eq!(
        common_parent(&scope, &specs, None, &starts, &auth).unwrap(),
        SECOND
    );
    assert!(common_parent(&scope, &specs, Some(FIRST), &starts, &auth).is_err());
    assert_eq!(
        common_parent(&scope, &specs, Some(SECOND), &starts, &auth).unwrap(),
        SECOND
    );
    assert!(common_parent(&scope, &[claude()], None, &starts, &[]).is_err());
    let project = scope
        .projects_root
        .join(project_slug(&scope.canonical_worktree));
    fs::remove_dir(project.join(SECOND)).unwrap();
    assert_eq!(
        common_parent(&scope, &[claude()], None, &starts, &[]).unwrap(),
        FIRST
    );
}
