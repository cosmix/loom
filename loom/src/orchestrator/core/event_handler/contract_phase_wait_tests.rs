//! A contract writer that dies while its stage waits for input is replaced.

use serial_test::serial;

use super::{
    assigned_session, contract_session, handoff_work_dir, manual_orchestrator, track_worktree,
    v2_stage, HooksDirGuard,
};
use crate::models::session::SessionType;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{create_stage, load_stage};

#[test]
#[serial]
fn a_dead_contract_writer_of_a_waiting_stage_is_replaced() {
    let _hooks = HooksDirGuard::install();
    let temp = handoff_work_dir();
    let work = temp.path().join(".loom").join("work");
    let first = contract_session(&work);
    create_stage(
        &v2_stage(StageStatus::WaitingForInput, Some(&first.id)),
        &work,
    )
    .unwrap();
    let mut orchestrator = manual_orchestrator(&work, temp.path());
    orchestrator.graph.mark_executing("test-stage").unwrap();
    track_worktree(&mut orchestrator, temp.path());

    orchestrator
        .on_contract_session_ended("test-stage", &first.id)
        .unwrap();

    let stage = load_stage("test-stage", &work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    let replacement = assigned_session(&work, &stage);
    assert_ne!(replacement.id, first.id);
    assert_eq!(replacement.session_type, SessionType::Contract);
}
