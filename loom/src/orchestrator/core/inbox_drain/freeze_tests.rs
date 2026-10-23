//! A relayed freeze the daemon refuses is recorded for adjudication; the
//! stage keeps executing.

use std::path::PathBuf;

use tempfile::TempDir;

use super::super::Settle;
use super::settle;
use crate::daemon::Response;
use crate::models::stage::StageStatus;
use crate::verify::contracts::refusal::{daemon_refusal, RefusalSource};
use crate::verify::contracts::test_support::contract_stage;
use crate::verify::transitions::{create_stage, load_stage, update_stage};

const STAGE: &str = "s1";
const OWNER: &str = "writer-1";
const REFUSAL: &str = "src/lib.rs is not a contract file\nformat check failed";

fn work_dir() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    create_stage(&contract_stage(STAGE, OWNER), &work).unwrap();
    (tmp, work)
}

fn refuse(work: &std::path::Path) -> Settle {
    let refused = Ok(Response::Error {
        message: REFUSAL.to_string(),
    });
    settle(work, STAGE, OWNER, "r1", refused)
}

#[test]
fn a_refused_relayed_freeze_is_recorded_and_the_stage_keeps_executing() {
    let (_tmp, work) = work_dir();

    assert_eq!(refuse(&work), Settle::Refused(REFUSAL.to_string()));

    let stage = load_stage(STAGE, &work).unwrap();
    assert_eq!(
        stage.status,
        StageStatus::Executing,
        "never WaitingForInput"
    );
    assert_eq!(stage.review_reason, None);
    let record = daemon_refusal(&work, STAGE).unwrap().unwrap();
    assert_eq!(record.session_id, OWNER);
    assert_eq!(record.source, RefusalSource::Daemon);
    assert_eq!(
        record.problems,
        ["src/lib.rs is not a contract file", "format check failed"]
    );
}

#[test]
fn a_refusal_of_a_stage_that_is_not_executing_records_nothing() {
    let (_tmp, work) = work_dir();
    update_stage(STAGE, &work, |stage| stage.try_mark_waiting_for_input()).unwrap();

    assert_eq!(refuse(&work), Settle::Refused(REFUSAL.to_string()));

    assert_eq!(daemon_refusal(&work, STAGE).unwrap(), None);
    let stage = load_stage(STAGE, &work).unwrap();
    assert_eq!(stage.status, StageStatus::WaitingForInput);
}

#[test]
fn a_refusal_that_cannot_be_recorded_says_so() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: `geteuid` has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let (_tmp, work) = work_dir();
    let dir = work.join("contracts").join(STAGE);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();

    let settled = refuse(&work);

    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let Settle::Refused(reason) = settled else {
        panic!("expected a refusal, got {settled:?}");
    };
    assert!(reason.starts_with("could not be applied: "), "{reason}");
    assert!(
        reason.contains("src/lib.rs is not a contract file"),
        "{reason}"
    );
    assert_eq!(daemon_refusal(&work, STAGE).unwrap(), None);
}
