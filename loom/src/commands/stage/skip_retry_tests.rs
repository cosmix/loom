//! Tests for what a retry does to a v2 stage's contract respawn budget.

use tempfile::TempDir;

use super::*;
use crate::verify::contracts::store::{attempts_spent, spend_attempt};
use crate::verify::contracts::test_support::{contract_stage, write_test_freeze};
use crate::verify::transitions::create_stage;

/// A blocked v2 stage whose contract writers spent the whole budget, and the
/// state `retry` plans for it.
fn spent_contract_stage(work_dir: &Path) -> crate::models::stage::Stage {
    let mut stage = contract_stage("s1", "writer-1");
    stage.status = StageStatus::Blocked;
    create_stage(&stage, work_dir).unwrap();
    for _ in 0..3 {
        spend_attempt(work_dir, "s1").unwrap();
    }
    stage.session = None;
    stage
}

#[test]
fn retry_grants_an_unfrozen_stage_a_fresh_contract_budget() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    persist_retry_delta("s1", temp.path(), &planned, &StageStatus::Blocked, false).unwrap();

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 0);
}

#[test]
fn retry_leaves_a_frozen_stage_budget_alone() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());
    write_test_freeze(temp.path(), "s1", "writer-1");

    persist_retry_delta("s1", temp.path(), &planned, &StageStatus::Blocked, false).unwrap();

    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Queued
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 3);
}

/// A refused transition (the on-disk status no longer matches what `retry`
/// planned against) must not reset the budget: the reset sits inside the
/// same locked closure, after the status re-validation, so its error path
/// is never reached.
#[test]
fn retry_refused_transition_leaves_budget_unspent() {
    let temp = TempDir::new().unwrap();
    let planned = spent_contract_stage(temp.path());

    // The stage is on disk as `Blocked`, but `retry` is told to expect
    // `Executing` — the closure's own re-validation refuses the mismatch.
    let result = persist_retry_delta("s1", temp.path(), &planned, &StageStatus::Executing, false);

    assert!(result.is_err());
    assert_eq!(
        load_stage("s1", temp.path()).unwrap().status,
        StageStatus::Blocked
    );
    assert_eq!(attempts_spent(temp.path(), "s1").unwrap(), 3);
}
