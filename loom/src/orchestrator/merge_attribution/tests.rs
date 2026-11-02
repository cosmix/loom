use super::*;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::models::stage::{Stage, StageStatus, StageType};
use std::process::Command;
use tempfile::TempDir;

fn run_git(args: &[&str], cwd: &Path) -> std::process::Output {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn init_repo() -> TempDir {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    run_git(&["init", "-b", "main"], root);
    run_git(&["config", "user.email", "t@t.com"], root);
    run_git(&["config", "user.name", "t"], root);
    std::fs::write(root.join("README.md"), "seed").unwrap();
    run_git(&["add", "README.md"], root);
    run_git(&["commit", "-m", "seed"], root);
    tmp
}

fn make_stage(id: &str) -> Stage {
    let mut s = Stage::new(id.to_string(), Some("test".to_string()));
    s.id = id.to_string();
    s.stage_type = StageType::Standard;
    s.status = StageStatus::Completed;
    s
}

fn create_conflict_in_main(repo_root: &Path, stage_id: &str) {
    let branch = branch_name_for_stage(stage_id);
    run_git(&["checkout", "-b", &branch], repo_root);
    std::fs::write(repo_root.join("file.txt"), "branch\n").unwrap();
    run_git(&["add", "file.txt"], repo_root);
    run_git(&["commit", "-m", "branch"], repo_root);
    run_git(&["checkout", "main"], repo_root);
    std::fs::write(repo_root.join("file.txt"), "main\n").unwrap();
    run_git(&["add", "file.txt"], repo_root);
    run_git(&["commit", "-m", "main"], repo_root);
    // Try to merge -- creates MERGE_HEAD with conflicts.
    let _ = Command::new("git")
        .args(["merge", "--no-ff", &branch])
        .current_dir(repo_root)
        .output()
        .unwrap();
}

#[test]
fn no_active_merge_returns_none() {
    let repo = init_repo();
    let result = attribute_main_repo_merge(repo.path(), repo.path(), &[], &[]).unwrap();
    assert_eq!(result, MergeAttribution::None);
}

#[test]
fn branch_head_match_attributes_to_correct_stage() {
    let repo = init_repo();
    create_conflict_in_main(repo.path(), "stage-a");

    let stage = make_stage("stage-a");
    let result = attribute_main_repo_merge(repo.path(), repo.path(), &[stage], &[]).unwrap();

    match result {
        MergeAttribution::Attributed {
            stage_id, source, ..
        } => {
            assert_eq!(stage_id, "stage-a");
            assert_eq!(source, AttributionSource::BranchHeadMatch);
        }
        other => panic!("expected Attributed, got {other:?}"),
    }
}

#[test]
fn unrelated_stage_b_not_attributed_when_stage_a_is_active() {
    let repo = init_repo();
    create_conflict_in_main(repo.path(), "stage-a");

    let stage_a = make_stage("stage-a");
    let stage_b = make_stage("stage-b"); // no branch
    let result =
        attribute_main_repo_merge(repo.path(), repo.path(), &[stage_b, stage_a], &[]).unwrap();

    match result {
        MergeAttribution::Attributed { stage_id, .. } => {
            assert_eq!(stage_id, "stage-a", "must NOT attribute to stage-b");
        }
        other => panic!("expected Attributed for stage-a, got {other:?}"),
    }
}

#[test]
fn merge_session_attribution_via_metadata() {
    let repo = init_repo();
    create_conflict_in_main(repo.path(), "feat-x");

    let stage = make_stage("feat-x");
    let mut session = Session::new();
    session.id = "merge-session-1".to_string();
    session.status = SessionStatus::Crashed;
    session.session_type = SessionType::Merge;
    session.merge_source_branch = Some("loom/feat-x".to_string());
    session.merge_target_branch = Some("main".to_string());

    let result = attribute_main_repo_merge(repo.path(), repo.path(), &[stage], &[session]).unwrap();

    match result {
        MergeAttribution::Attributed {
            stage_id, source, ..
        } => {
            assert_eq!(stage_id, "feat-x");
            match source {
                AttributionSource::MergeSession { session_id } => {
                    assert_eq!(session_id, "merge-session-1");
                }
                other => panic!("expected MergeSession source, got {other:?}"),
            }
        }
        other => panic!("expected Attributed, got {other:?}"),
    }
}

#[test]
fn stale_merge_session_cannot_steal_another_stages_merge() {
    let repo = init_repo();
    run_git(&["branch", "loom/stage-a"], repo.path());
    run_git(&["branch", "-D", "loom/stage-a"], repo.path());
    create_conflict_in_main(repo.path(), "stage-b");

    let mut old = Session::new();
    old.id = "old-merge-a".to_string();
    old.status = SessionStatus::Crashed;
    old.session_type = SessionType::Merge;
    old.merge_source_branch = Some("loom/stage-a".to_string());
    old.merge_target_branch = Some("main".to_string());

    let result = attribute_main_repo_merge(
        repo.path(),
        repo.path(),
        &[make_stage("stage-a"), make_stage("stage-b")],
        &[old],
    )
    .unwrap();
    assert!(matches!(
        result,
        MergeAttribution::Attributed {
            stage_id,
            source: AttributionSource::BranchHeadMatch,
            ..
        } if stage_id == "stage-b"
    ));
}

#[test]
fn reconciliation_never_reopens_stage_from_stale_merge_metadata() {
    let repo = init_repo();
    let root = repo.path();
    run_git(&["branch", "loom/stage-a"], root);
    run_git(&["branch", "-D", "loom/stage-a"], root);
    create_conflict_in_main(root, "stage-b");
    let work = root.join(".loom").join("work");
    crate::verify::transitions::save_stage(&make_stage("stage-a"), &work).unwrap();
    crate::verify::transitions::save_stage(&make_stage("stage-b"), &work).unwrap();

    let mut old = Session::new();
    old.id = "old-merge-a".to_string();
    old.status = SessionStatus::Completed;
    old.session_type = SessionType::Merge;
    old.merge_source_branch = Some("loom/stage-a".to_string());
    crate::fs::session_files::save_session(&old, &work).unwrap();

    let outcome = reconcile_main_repo_active_merge(root, &work).unwrap();
    assert!(matches!(
        outcome,
        ReconciliationOutcome::StageMutated { stage_id, .. } if stage_id == "stage-b"
    ));
    assert_eq!(
        crate::verify::transitions::load_stage("stage-a", &work)
            .unwrap()
            .status,
        StageStatus::Completed
    );
}

#[test]
fn failed_source_lookup_without_exact_evidence_is_unattributed() {
    let repo = init_repo();
    create_conflict_in_main(repo.path(), "stage-b");
    run_git(&["branch", "-D", "loom/stage-b"], repo.path());

    let mut old = Session::new();
    old.id = "old-merge-a".to_string();
    old.status = SessionStatus::Completed;
    old.session_type = SessionType::Merge;
    old.merge_source_branch = Some("loom/stage-a".to_string());
    let result = attribute_main_repo_merge(
        repo.path(),
        repo.path(),
        &[make_stage("stage-a"), make_stage("stage-b")],
        &[old],
    )
    .unwrap();
    assert!(matches!(result, MergeAttribution::GlobalUnattributed(_)));
}

#[test]
fn git_lookup_error_does_not_turn_session_into_a_wildcard() {
    let repo = init_repo();
    let root = repo.path();
    create_conflict_in_main(root, "stage-b");
    let bad_ref = root.join(".git").join("refs/heads/loom/stage-a");
    std::fs::write(&bad_ref, "invalid-object-id\n").unwrap();
    assert!(get_branch_head("loom/stage-a", root).is_err());

    let mut old = Session::new();
    old.id = "old-merge-a".to_string();
    old.session_type = SessionType::Merge;
    old.merge_source_branch = Some("loom/stage-a".to_string());
    let result = attribute_main_repo_merge(
        root,
        root,
        &[make_stage("stage-a"), make_stage("stage-b")],
        &[old],
    )
    .unwrap();
    assert!(matches!(
        result,
        MergeAttribution::Attributed { stage_id, .. } if stage_id == "stage-b"
    ));
}

#[test]
fn completed_commit_match_attributes_when_no_branch() {
    let repo = init_repo();
    let root = repo.path();

    // Build a branch and remember its HEAD as completed_commit, then
    // delete the branch. Recreate as a "loom/<id>" with same SHA so we
    // can produce MERGE_HEAD pointing at it.
    run_git(&["checkout", "-b", "loom/stage-c"], root);
    std::fs::write(root.join("c.txt"), "branch\n").unwrap();
    run_git(&["add", "c.txt"], root);
    run_git(&["commit", "-m", "c"], root);
    let stranded_sha = String::from_utf8_lossy(&run_git(&["rev-parse", "HEAD"], root).stdout)
        .trim()
        .to_string();

    run_git(&["checkout", "main"], root);
    std::fs::write(root.join("c.txt"), "main\n").unwrap();
    run_git(&["add", "c.txt"], root);
    run_git(&["commit", "-m", "main"], root);

    let _ = Command::new("git")
        .args(["merge", "--no-ff", "loom/stage-c"])
        .current_dir(root)
        .output()
        .unwrap();
    // Now delete the branch reference but MERGE_HEAD is set.
    run_git(&["branch", "-D", "loom/stage-c"], root);

    let mut stage = make_stage("stage-c");
    stage.completed_commit = Some(stranded_sha);

    let result = attribute_main_repo_merge(root, root, &[stage], &[]).unwrap();
    match result {
        MergeAttribution::Attributed {
            stage_id, source, ..
        } => {
            assert_eq!(stage_id, "stage-c");
            assert_eq!(source, AttributionSource::CompletedCommitMatch);
        }
        other => panic!("expected Attributed, got {other:?}"),
    }
}

#[test]
fn base_conflict_branch_carves_out_attribution() {
    let repo = init_repo();
    let root = repo.path();
    // Build a stage branch and bring conflict into a loom/_base branch.
    run_git(&["checkout", "-b", "loom/feat-y"], root);
    std::fs::write(root.join("y.txt"), "y\n").unwrap();
    run_git(&["add", "y.txt"], root);
    run_git(&["commit", "-m", "y"], root);

    run_git(&["checkout", "main"], root);
    run_git(&["checkout", "-b", "loom/_base/composite"], root);
    std::fs::write(root.join("y.txt"), "main\n").unwrap();
    run_git(&["add", "y.txt"], root);
    run_git(&["commit", "-m", "base seed"], root);

    let _ = Command::new("git")
        .args(["merge", "--no-ff", "loom/feat-y"])
        .current_dir(root)
        .output()
        .unwrap();

    let stage = make_stage("feat-y");
    let result = attribute_main_repo_merge(root, root, &[stage], &[]).unwrap();
    match result {
        MergeAttribution::GlobalUnattributed(_) => {}
        other => panic!("expected GlobalUnattributed (BaseConflict carve-out), got {other:?}"),
    }
}
