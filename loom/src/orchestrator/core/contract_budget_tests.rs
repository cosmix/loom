//! Daemon-restart recovery of a dead contract session against the respawn
//! budget: the requeue `recover_orphaned_sessions` makes hands out a fresh
//! contract writer, so it must be charged as `ContractSessionEnded` would
//! have charged it under a watching daemon.

use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{SessionBackendKind, SessionStatus, TerminalConfig};
use crate::orchestrator::core::recovery::Recovery;
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::liveness::LivenessService;
use crate::plan::schema::StageDefinition;
use crate::plan::ExecutionGraph;
use crate::verify::contracts::store::{attempts_spent, spend_attempt};
use crate::verify::contracts::test_support::{contract_stage, write_test_freeze};
use crate::verify::transitions::{create_stage, load_stage};

const STAGE: &str = "s1";

/// A `.loom/work` holding a v2 stage at `status` whose current contract
/// session is old enough for a failed liveness probe to count, with `spent`
/// replacement writers already charged.
fn dead_contract_writer(status: StageStatus, spent: u32) -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let work = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    // The tmux lane keeps `Orchestrator::new` off terminal detection, which
    // fails on a headless runner.
    let terminal = TerminalConfig {
        backend: SessionBackendKind::Tmux,
    };
    write_terminal_config(&work, &terminal).unwrap();
    let mut writer = Session::new_contract(STAGE);
    writer.status = SessionStatus::Running;
    writer.backend = SessionBackendKind::Tmux;
    writer.created_at = chrono::Utc::now() - chrono::Duration::minutes(5);
    save_session(&writer, &work).unwrap();
    let mut stage = contract_stage(STAGE, &writer.id);
    stage.status = status;
    create_stage(&stage, &work).unwrap();
    for _ in 0..spent {
        spend_attempt(&work, STAGE).unwrap();
    }
    (temp, work)
}

/// Run the orphan pass a restarted daemon runs, every agent probed dead, and
/// return the stage it leaves.
fn recover_after_restart(temp: &TempDir, work: &Path) -> Stage {
    recover_after_restart_in_mode(temp, work, false)
}

/// [`recover_after_restart`], with the orphan pass run as `loom run --manual`
/// would run it.
fn recover_after_restart_in_mode(temp: &TempDir, work: &Path, manual_mode: bool) -> Stage {
    let config = OrchestratorConfig {
        work_dir: work.to_path_buf(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        manual_mode,
        ..Default::default()
    };
    let graph = ExecutionGraph::build(vec![StageDefinition {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        working_dir: ".".to_string(),
        ..Default::default()
    }])
    .unwrap();
    let mut orchestrator = Orchestrator::new(config, graph).unwrap();
    orchestrator.liveness = LivenessService::fixed_for_tests(false);

    assert_eq!(orchestrator.recover_orphaned_sessions().unwrap(), 1);
    load_stage(STAGE, work).unwrap()
}

#[test]
fn a_dead_writer_below_budget_is_charged_and_requeued() {
    let (temp, work) = dead_contract_writer(StageStatus::Executing, 1);

    let stage = recover_after_restart(&temp, &work);

    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.session, None);
    assert_eq!(
        attempts_spent(&work, STAGE).unwrap(),
        2,
        "the requeue hands out a replacement writer, so it is charged"
    );
}

#[test]
fn a_dead_writer_at_budget_waits_for_a_human() {
    let (temp, work) = dead_contract_writer(StageStatus::Executing, MAX_CONTRACT_RESPAWNS);

    let stage = recover_after_restart(&temp, &work);

    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert_eq!(
        stage.review_reason.as_deref(),
        Some("contract session ended 3 times without freezing contracts")
    );
    assert_eq!(stage.session, None);
    assert_eq!(attempts_spent(&work, STAGE).unwrap(), MAX_CONTRACT_RESPAWNS);
}

#[test]
fn a_dead_writer_that_froze_requeues_uncharged() {
    let (temp, work) = dead_contract_writer(StageStatus::Executing, 0);
    write_test_freeze(&work, STAGE, "writer");

    let stage = recover_after_restart(&temp, &work);

    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(
        attempts_spent(&work, STAGE).unwrap(),
        0,
        "with the contracts frozen the next spawn is the implementer"
    );
}

/// A writer at its context ceiling hands off; a watching daemon continues it
/// with a fresh writer uncharged, and so does recovery.
#[test]
fn a_writer_that_handed_off_is_continued_uncharged() {
    let (temp, work) = dead_contract_writer(StageStatus::NeedsHandoff, 0);

    let stage = recover_after_restart(&temp, &work);

    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(attempts_spent(&work, STAGE).unwrap(), 0);
}

/// In manual mode loom never spawned the writer, so it has no PID-identity
/// evidence and the liveness probe reports it dead unconditionally. A
/// dead-looking-but-possibly-alive writer below budget must still requeue
/// the stage, but must not spend an attempt: charging one on every `loom run
/// --manual` restart would escalate to NeedsHumanReview within three
/// restarts regardless of whether the operator's writer is still running.
#[test]
fn a_manual_mode_dead_looking_writer_below_budget_is_requeued_uncharged() {
    let (temp, work) = dead_contract_writer(StageStatus::Executing, 1);

    let stage = recover_after_restart_in_mode(&temp, &work, true);

    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.session, None);
    assert_eq!(
        attempts_spent(&work, STAGE).unwrap(),
        1,
        "manual mode must not charge a respawn it cannot prove was needed"
    );
}

/// A `Blocked` stage has no edge to `NeedsHumanReview`, so
/// `charge_orphaned_contract_writer` only ever charges an `Executing`
/// stage's writer: recovery requeues a dead `Blocked` writer uncharged.
#[test]
fn a_dead_writer_on_a_blocked_stage_requeues_uncharged() {
    let (temp, work) = dead_contract_writer(StageStatus::Blocked, 0);

    let stage = recover_after_restart(&temp, &work);

    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(attempts_spent(&work, STAGE).unwrap(), 0);
}

/// The restart harness runs with no git repository, so `commits_ahead` is
/// always 0 there and never exercises the handoff route; call
/// `recover_orphaned_stage` directly to pin it. A contract writer that died
/// unfrozen spends an attempt whichever route its successor takes, matching
/// the live `ContractSessionEnded` path.
#[test]
fn a_dead_unfrozen_writer_routed_to_handoff_is_charged() {
    let (_temp, work) = dead_contract_writer(StageStatus::Executing, 0);
    let mut stage = load_stage(STAGE, &work).unwrap();
    let session_id = stage.session.clone().unwrap();
    let session = load_session_exact(&work, &session_id).unwrap().unwrap();

    let recovered = super::super::orphan_adoption::recover_orphaned_stage(
        &mut stage, &session, &work, 1, "main", false,
    );

    assert!(recovered);
    assert_eq!(stage.status, StageStatus::NeedsHandoff);
    assert_eq!(attempts_spent(&work, STAGE).unwrap(), 1);
}
