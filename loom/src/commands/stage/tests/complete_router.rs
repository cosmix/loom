//! Unit tests for `route_complete_for_conflicts` — the pure routing helper
//! that replaces the bare `complete()` `--force-unsafe` and conflict logic.

use std::path::Path;

use super::super::complete::{route_complete_for_conflicts, CompleteConflictRoute};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::test_git::{git, TestRepo};

fn make_work_dir(repo: &Path) -> std::path::PathBuf {
    let work_dir = repo.join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    std::fs::write(work_dir.join("config.toml"), "base_branch = \"main\"\n").unwrap();
    work_dir
}

fn make_stage(id: &str, status: StageStatus) -> Stage {
    let mut s = Stage::new(id.to_string(), Some(format!("test {id}")));
    s.id = id.to_string();
    s.stage_type = StageType::Standard;
    s.status = status;
    s
}

#[test]
fn knowledge_stage_proceeds_regardless() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let work_dir = make_work_dir(repo.path());
    let mut stage = make_stage("kb", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();
    assert_eq!(route, CompleteConflictRoute::Proceed);
}

#[test]
fn merge_conflict_with_daemon_running_returns_daemon_managed() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let work_dir = make_work_dir(repo.path());
    let stage = make_stage("st", StageStatus::MergeConflict);
    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        true, // daemon running
        false,
        false,
    )
    .unwrap();
    assert_eq!(
        route,
        CompleteConflictRoute::DaemonManaged {
            stage_id: "st".to_string()
        }
    );
}

#[test]
fn merge_conflict_with_daemon_off_returns_spawn_resolver() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let work_dir = make_work_dir(repo.path());
    let stage = make_stage("st", StageStatus::MergeConflict);
    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();
    match route {
        CompleteConflictRoute::SpawnResolver { target_branch, .. } => {
            assert_eq!(target_branch, "main");
        }
        other => panic!("expected SpawnResolver, got {other:?}"),
    }
}

#[test]
fn merge_blocked_routes_same_as_merge_conflict() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let work_dir = make_work_dir(repo.path());
    let stage = make_stage("st", StageStatus::MergeBlocked);
    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();
    assert!(matches!(route, CompleteConflictRoute::SpawnResolver { .. }));
}

#[test]
fn force_unsafe_assume_merged_with_ancestor_returns_verified() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let root = repo.path();
    let work_dir = make_work_dir(root);
    git(root, &["checkout", "-b", "loom/feat"]);
    std::fs::write(root.join("a.rs"), "ok").unwrap();
    git(root, &["add", "a.rs"]);
    git(root, &["commit", "-m", "feat"]);
    git(root, &["checkout", "main"]);
    git(root, &["merge", "--no-ff", "-m", "merge", "loom/feat"]);

    let head = crate::git::branch::get_branch_head("loom/feat", root).unwrap();
    let mut stage = make_stage("feat", StageStatus::Completed);
    stage.completed_commit = Some(head.clone());

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        true,
        true,
    )
    .unwrap();
    match route {
        CompleteConflictRoute::ForceUnsafeAssumeMergedVerified { derived_commit } => {
            assert!(derived_commit.is_none(), "no derivation needed");
        }
        other => panic!("expected ForceUnsafeAssumeMergedVerified, got {other:?}"),
    }
}

#[test]
fn force_unsafe_assume_merged_with_no_ancestor_refuses() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let root = repo.path();
    let work_dir = make_work_dir(root);
    git(root, &["checkout", "-b", "loom/stranded"]);
    std::fs::write(root.join("a.rs"), "ok").unwrap();
    git(root, &["add", "a.rs"]);
    git(root, &["commit", "-m", "stranded"]);
    git(root, &["checkout", "main"]);

    let head = crate::git::branch::get_branch_head("loom/stranded", root).unwrap();
    let mut stage = make_stage("stranded", StageStatus::Completed);
    stage.completed_commit = Some(head);

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        true,
        true,
    )
    .unwrap();
    assert!(matches!(route, CompleteConflictRoute::Refuse { .. }));
}

#[test]
fn force_unsafe_assume_merged_derives_when_completed_commit_missing_and_branch_merged() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let root = repo.path();
    let work_dir = make_work_dir(root);
    git(root, &["checkout", "-b", "loom/derive"]);
    std::fs::write(root.join("a.rs"), "ok").unwrap();
    git(root, &["add", "a.rs"]);
    git(root, &["commit", "-m", "feat"]);
    git(root, &["checkout", "main"]);
    git(root, &["merge", "--no-ff", "-m", "merge", "loom/derive"]);

    let mut stage = make_stage("derive", StageStatus::Completed);
    stage.completed_commit = None;

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        true,
        true,
    )
    .unwrap();
    match route {
        CompleteConflictRoute::ForceUnsafeAssumeMergedVerified { derived_commit } => {
            assert!(derived_commit.is_some(), "router must signal derivation");
        }
        other => panic!("expected ForceUnsafeAssumeMergedVerified, got {other:?}"),
    }
}

#[test]
fn force_unsafe_assume_merged_refuses_when_branch_missing_and_no_commit() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let work_dir = make_work_dir(repo.path());
    let stage = make_stage("ghost", StageStatus::Completed);
    // No branch, no completed_commit.
    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        false,
        true,
        true,
    )
    .unwrap();
    assert!(matches!(route, CompleteConflictRoute::Refuse { .. }));
}

#[test]
fn force_unsafe_alone_with_no_active_merge_returns_stale_flag_route() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let work_dir = make_work_dir(repo.path());
    let stage = make_stage("st", StageStatus::CompletedWithFailures);

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        repo.path(),
        &work_dir,
        false,
        true,
        false,
    )
    .unwrap();
    assert_eq!(route, CompleteConflictRoute::ForceUnsafeAllowedStaleFlag);
}

#[test]
fn refusal_preserves_stage_state_on_disk() {
    // Routing is read-only by construction. This test verifies that the
    // helper does not write to the stage file even when refusing.
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let root = repo.path();
    let work_dir = make_work_dir(root);
    git(root, &["checkout", "-b", "loom/stranded2"]);
    std::fs::write(root.join("c.rs"), "ok").unwrap();
    git(root, &["add", "c.rs"]);
    git(root, &["commit", "-m", "c"]);
    git(root, &["checkout", "main"]);

    let head = crate::git::branch::get_branch_head("loom/stranded2", root).unwrap();
    let mut stage = make_stage("stranded2", StageStatus::Completed);
    stage.completed_commit = Some(head.clone());
    crate::verify::transitions::save_stage(&stage, &work_dir).unwrap();

    let _ = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        true,
        true,
    )
    .unwrap();

    // Refusal must NOT have modified the stage file. Reload and check.
    let after = crate::verify::transitions::load_stage("stranded2", &work_dir).unwrap();
    assert_eq!(after.status, StageStatus::Completed);
    assert_eq!(after.completed_commit, Some(head));
    assert!(!after.merged);
}

#[test]
fn unattributed_main_repo_merge_is_refused() {
    // Manufacture a MERGE_HEAD with a SHA that no stage knows about.
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let root = repo.path();
    let work_dir = make_work_dir(root);
    let bogus_sha = "deadbeefcafebabe1234567890abcdef12345678";
    std::fs::write(
        root.join(".git").join("MERGE_HEAD"),
        format!("{bogus_sha}\n"),
    )
    .unwrap();

    let stage = make_stage("st", StageStatus::Executing);

    let route = route_complete_for_conflicts(
        &stage,
        &[],
        std::slice::from_ref(&stage),
        root,
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();
    assert!(matches!(route, CompleteConflictRoute::Refuse { .. }));

    // Cleanup so we don't leak across tests on shared repo.
    std::fs::remove_file(root.join(".git").join("MERGE_HEAD")).ok();
}

#[test]
fn other_stage_merge_active_refuses_for_this_stage() {
    let repo = TestRepo::builder()
        .seed("seed.txt", "seed", "seed")
        .create();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    // Create a conflict with stage-a in main repo.
    git(root, &["checkout", "-b", "loom/stage-a"]);
    std::fs::write(root.join("file.txt"), "branch\n").unwrap();
    git(root, &["add", "file.txt"]);
    git(root, &["commit", "-m", "branch"]);
    git(root, &["checkout", "main"]);
    std::fs::write(root.join("file.txt"), "main\n").unwrap();
    git(root, &["add", "file.txt"]);
    git(root, &["commit", "-m", "main"]);
    let _ = std::process::Command::new("git")
        .args(["merge", "--no-ff", "loom/stage-a"])
        .current_dir(root)
        .output();

    let stage_a = make_stage("stage-a", StageStatus::Executing);
    let stage_b = make_stage("stage-b", StageStatus::Executing);

    // Routing for stage-b: refusal because stage-a owns the active merge.
    let route = route_complete_for_conflicts(
        &stage_b,
        &[],
        &[stage_a, stage_b.clone()],
        root,
        &work_dir,
        false,
        false,
        false,
    )
    .unwrap();
    assert!(matches!(route, CompleteConflictRoute::Refuse { .. }));
}

#[path = "complete_router_worktree.rs"]
mod worktree;
