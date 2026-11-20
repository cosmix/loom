//! Removing a judge's heartbeat when the judge is closed.

use super::*;

/// Closing a judge removes its heartbeat, and a judge closed before its first
/// tool call never wrote one — so a missing file is success, not an error.
#[test]
fn cleaning_up_a_judge_heartbeat_tolerates_a_missing_file() {
    let temp = tempfile::TempDir::new().unwrap();
    let work_dir = temp.path();
    write_judge_heartbeat(work_dir, "disputed-stage", "judge-1", 0);
    let path = judge_heartbeat_path(work_dir, "disputed-stage");
    assert!(
        path.exists(),
        "fixture heartbeat must exist to be deletable"
    );

    cleanup_judge_heartbeat(work_dir, "disputed-stage");
    assert!(!path.exists());

    cleanup_judge_heartbeat(work_dir, "disputed-stage");
    cleanup_judge_heartbeat(work_dir, "never-existed");
}
