use super::*;
use crate::commands::stage::tests::complete::operator_landing::{commit_in_worktree, parked_stage};
use crate::verify::transitions::load_stage;
use serial_test::serial;

/// The tip recorded for landing is re-validated under the stage lock. When
/// the branch moved after the capture, the write is refused and the stage
/// keeps its state; `report_forced_landing` runs only after a successful
/// write, so no recorded commit is announced for it.
#[test]
#[serial]
fn a_tip_that_moved_before_the_locked_write_completes_nothing() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    let stage = load_stage("test-stage", &work_dir).unwrap();
    let stale = VerificationSnapshot::capture(&stage, &work_dir).unwrap();
    commit_in_worktree(temp_dir.path(), "moved on");

    let landing = ForcedLanding::Unmerged(Some(stale));
    let error = handle_force_unsafe_completion(&stage, landing, &work_dir).unwrap_err();

    assert!(
        format!("{error:#}").contains("completion content changed"),
        "{error:#}"
    );
    let stage = load_stage("test-stage", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(!stage.merged);
    assert_eq!(stage.completed_commit, None);
}
