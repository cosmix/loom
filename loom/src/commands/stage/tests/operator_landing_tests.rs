//! The operator escape hatches land a parked stage whose completion never
//! recorded a commit: `human-review --force-complete` merges the branch tip,
//! and `complete --force-unsafe` records it for the daemon's auto-merge.

use super::*;
use crate::commands::stage::human_review;
use crate::commands::stage::progressive_complete::branch_work;

/// A git repo whose `loom/test-stage` worktree is registered, `advance`d by
/// one commit or not, and the stage saved with `status` and no recorded
/// commit. Returns the temp dir and the state directory.
pub(in crate::commands::stage) fn parked_stage(
    status: StageStatus,
    advance: bool,
) -> (TempDir, std::path::PathBuf) {
    let temp_dir = setup_work_dir();
    let work_dir = temp_dir.path().join(".loom").join("work");
    initialize_zero_commit_repo(temp_dir.path());
    let mut stage = create_test_stage("test-stage", status);
    stage.review_reason = Some("completion proof is missing".into());
    register_stage_worktree(temp_dir.path(), &mut stage, advance);
    save_test_stage(&work_dir, &stage);
    (temp_dir, work_dir)
}

fn branch_tip(repo: &Path) -> String {
    crate::git::get_branch_head("loom/test-stage", repo).unwrap()
}

/// Commit `content` to README.md on the `loom/test-stage` worktree, moving
/// its tip.
pub(in crate::commands::stage) fn commit_in_worktree(repo: &Path, content: &str) {
    let worktree = repo.join(".worktrees").join("test-stage");
    std::fs::write(worktree.join("README.md"), content).unwrap();
    for args in [
        ["add", "README.md"].as_slice(),
        ["commit", "-m", content].as_slice(),
    ] {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&worktree)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    }
}

/// Assert the reviewed stage is exactly as `parked_stage` saved it.
fn assert_still_parked(work_dir: &Path) {
    let stage = load_stage("test-stage", work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit, None);
}

#[test]
#[serial]
fn force_complete_lands_a_committed_branch_without_a_recorded_commit() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    let tip = branch_tip(temp_dir.path());
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = human_review("test-stage".into(), false, true, None);

    assert!(result.is_ok(), "{result:?}");
    let stage = load_stage("test-stage", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert_eq!(stage.completed_commit.as_deref(), Some(tip.as_str()));
}

#[test]
#[serial]
fn force_complete_refuses_a_zero_commit_branch() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, false);
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let error = human_review("test-stage".into(), false, true, None).unwrap_err();

    assert!(format!("{error:#}").contains("zero commits"), "{error:#}");
    assert_still_parked(&work_dir);
}

/// The merge bypass skips the zero-commit guard, so a branch whose commits
/// beyond the target cannot be counted (here: the target does not exist)
/// must be refused before anything is recorded.
#[test]
#[serial]
fn force_complete_refuses_when_the_ahead_count_fails() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    std::fs::write(
        work_dir.join("config.toml"),
        "[plan]\nbase_branch = \"absent-target\"\n",
    )
    .unwrap();
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let error = human_review("test-stage".into(), false, true, None).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("cannot confirm"), "{message}");
    assert_still_parked(&work_dir);
}

#[test]
#[serial]
fn force_complete_refuses_a_dirty_worktree_and_changes_nothing() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    let tip = branch_tip(temp_dir.path());
    let worktree = temp_dir.path().join(".worktrees").join("test-stage");
    std::fs::write(worktree.join("README.md"), "uncommitted").unwrap();
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let error = human_review("test-stage".into(), false, true, None).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("cannot record the tip"), "{message}");
    assert_still_parked(&work_dir);
    assert_eq!(branch_tip(temp_dir.path()), tip);
}

/// A git failure while probing for the branch (here: no repository at all)
/// is an error, never a missing branch that completes with nothing recorded.
#[test]
#[serial]
fn branch_work_propagates_a_failed_branch_probe() {
    let (_temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    let outside = TempDir::new().unwrap();
    let stage = load_stage("test-stage", &work_dir).unwrap();

    let result = branch_work(&stage, outside.path(), &work_dir);

    let error = result
        .err()
        .expect("a failed branch probe must be an error");
    assert!(
        format!("{error:#}").contains("loom/test-stage"),
        "{error:#}"
    );
}

fn force_unsafe(nonce: &str) -> Result<()> {
    let proof = completion_proof("test-stage", false, true, false, nonce);
    complete(
        "test-stage".into(),
        None,
        false,
        true,
        false,
        false,
        Some(proof),
    )
}

#[test]
#[serial]
fn force_unsafe_records_the_branch_tip_and_leaves_the_stage_unmerged() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    write_admin_token(&work_dir, "admin-secret-token");
    let tip = branch_tip(temp_dir.path());
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = force_unsafe("force-unsafe-tip-01");

    assert!(result.is_ok(), "{result:?}");
    let stage = load_stage("test-stage", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit.as_deref(), Some(tip.as_str()));
}

#[test]
#[serial]
fn force_unsafe_records_nothing_for_a_zero_commit_branch() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, false);
    write_admin_token(&work_dir, "admin-secret-token");
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = force_unsafe("force-unsafe-empty1");

    assert!(result.is_ok(), "{result:?}");
    let stage = load_stage("test-stage", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit, None);
}
