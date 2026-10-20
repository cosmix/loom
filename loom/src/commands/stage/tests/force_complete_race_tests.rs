//! `human-review --force-complete` against the daemon's per-tick coherence
//! watchdog: while the landing does its slow work, the watchdog runs exactly
//! as the daemon runs it, and must find nothing to block.

use std::cell::Cell;
use std::rc::Rc;

use super::operator_landing::parked_stage;
use super::*;
use crate::commands::stage::human_review;
use crate::commands::stage::progressive_complete::landing::set_window_probe;
use crate::orchestrator::coherence::{
    block_incoherent_stage, executing_stage_incoherence, live_worker_sessions,
    load_assigned_session,
};

/// One watchdog pass over `stage_id`, as `reconcile_one_stage_path` makes it.
/// Returns whether it blocked the stage.
fn watchdog_pass(work_dir: &Path, stage_id: &str) -> bool {
    let stage = load_stage(stage_id, work_dir).unwrap();
    let assigned = load_assigned_session(work_dir, &stage).unwrap();
    let Some(reason) = executing_stage_incoherence(&stage, assigned.as_ref()) else {
        return false;
    };
    if !live_worker_sessions(work_dir, &stage).unwrap().is_empty() {
        return false;
    }
    block_incoherent_stage(work_dir, stage_id, &reason)
        .unwrap()
        .is_some()
}

/// Run one watchdog pass inside the next landing's window. The cell holds
/// `None` until the pass ran, then whether it blocked the stage.
fn watch_landing_window(work_dir: &Path, stage_id: &str) -> Rc<Cell<Option<bool>>> {
    let verdict = Rc::new(Cell::new(None));
    let (seen, work_dir, stage_id) = (
        verdict.clone(),
        work_dir.to_path_buf(),
        stage_id.to_string(),
    );
    set_window_probe(move || seen.set(Some(watchdog_pass(&work_dir, &stage_id))));
    verdict
}

#[test]
#[serial]
fn the_watchdog_finds_nothing_to_block_while_a_force_completion_merges() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();
    let blocked = watch_landing_window(&work_dir, "test-stage");

    let result = human_review("test-stage".into(), false, true, None);

    assert_eq!(
        blocked.get(),
        Some(false),
        "the watchdog blocked the landing"
    );
    assert!(result.is_ok(), "{result:?}");
    let stage = load_stage("test-stage", &work_dir).unwrap();
    assert_eq!(stage.status, StageStatus::Completed);
    assert!(stage.merged);
    assert!(stage.review_reason.is_none());
}

#[test]
#[serial]
fn the_watchdog_finds_nothing_to_block_while_a_knowledge_force_completion_lands() {
    let temp = setup_work_dir();
    initialize_zero_commit_repo(temp.path());
    let work_dir = temp.path().join(".loom/work");
    let mut stage = create_test_stage("knowledge-bootstrap", StageStatus::NeedsHumanReview);
    stage.stage_type = StageType::Knowledge;
    stage.review_reason = Some("Acceptance cannot run in the stage environment".into());
    save_test_stage(&work_dir, &stage);
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp.path()).unwrap();
    let blocked = watch_landing_window(&work_dir, &stage.id);

    let result = human_review(stage.id.clone(), false, true, None);

    assert_eq!(
        blocked.get(),
        Some(false),
        "the watchdog blocked the landing"
    );
    assert!(result.is_ok(), "{result:?}");
    let landed = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(landed.status, StageStatus::Completed);
    assert!(landed.merged);
}

/// The merge lands, then the completion record cannot be written (here
/// another writer blocked the stage under the landing): the error says the
/// work is merged and names the command that records it.
#[test]
#[serial]
fn a_failed_record_after_a_landed_merge_names_the_recovery() {
    let (temp_dir, work_dir) = parked_stage(StageStatus::NeedsHumanReview, true);
    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();
    let probe_dir = work_dir.clone();
    set_window_probe(move || {
        crate::verify::transitions::update_stage("test-stage", &probe_dir, |stage| {
            stage.force_status_with_reason(StageStatus::Blocked, "a concurrent writer");
            Ok(())
        })
        .unwrap();
    });

    let error = human_review("test-stage".into(), false, true, None).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("is merged into"), "{message}");
    assert!(
        message.contains("loom stage complete test-stage --force-unsafe --assume-merged"),
        "{message}"
    );
    let landed = std::process::Command::new("git")
        .args(["show", "main:README.md"])
        .current_dir(temp_dir.path())
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&landed.stdout), "implemented");
    assert!(!load_stage("test-stage", &work_dir).unwrap().merged);
}
