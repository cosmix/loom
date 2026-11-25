//! What an unverified completion reports about landing, run through
//! `complete_without_verification` with no daemon in the state directory.
use super::*;
use crate::commands::stage::tests::complete::operator_landing::parked_stage;
use crate::commands::stage::tests::{create_test_stage, save_test_stage};
use crate::models::stage::StageStatus;
use crate::verify::transitions::load_stage;

const NO_DAEMON_ROUTE: &str = "no daemon is running to merge it";
const NO_COMMIT: &str =
    "  → No commit recorded for landing: the stage cannot merge until a commit is recorded.";

#[test]
fn an_unverified_completion_names_how_a_recorded_commit_lands() {
    let note = landing_note("s1", false);

    let recorded = landing_line(note.clone(), true);
    let none = landing_line(note.clone(), false);

    assert_eq!(recorded, note);
    assert!(recorded.contains("`loom run` merges it"), "{recorded}");
    assert_eq!(none, NO_COMMIT);
}

/// The report of completing `stage_id` without verification in the
/// repository `parked_stage` built, and the stage as stored afterwards.
fn unverified_report(stage_id: &str, repo: &Path, work_dir: &Path) -> (String, Stage) {
    let mut stage = load_stage(stage_id, work_dir).unwrap();
    let mut out = Vec::new();

    complete_without_verification(&mut stage, stage_id, repo, work_dir, &mut out).unwrap();

    (
        String::from_utf8(out).unwrap(),
        load_stage(stage_id, work_dir).unwrap(),
    )
}

#[test]
fn a_recorded_commit_is_followed_by_the_no_daemon_landing_note() {
    let (temp, work_dir) = parked_stage(StageStatus::Executing, true);

    let (report, stored) = unverified_report("test-stage", temp.path(), &work_dir);

    assert!(
        report.contains("completed (skipped verification)"),
        "{report}"
    );
    assert!(report.contains(NO_DAEMON_ROUTE), "{report}");
    assert!(report.contains("`loom run` merges it"), "{report}");
    assert!(!report.contains(NO_COMMIT), "{report}");
    assert!(stored.completed_commit.is_some());
}

#[test]
fn no_recorded_commit_says_the_stage_cannot_merge() {
    // A stage whose branch was never created: the one unverified completion
    // that records nothing (a zero-commit branch is refused).
    let (temp, work_dir) = parked_stage(StageStatus::Executing, false);
    save_test_stage(
        &work_dir,
        &create_test_stage("no-branch", StageStatus::Executing),
    );

    let (report, stored) = unverified_report("no-branch", temp.path(), &work_dir);

    assert!(report.contains(NO_COMMIT), "{report}");
    assert!(!report.contains(NO_DAEMON_ROUTE), "{report}");
    assert_eq!(stored.completed_commit, None);
}
