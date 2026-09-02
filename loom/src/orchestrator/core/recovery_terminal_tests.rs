//! Unit tests for `stage_file_is_terminal` and the daemon-shutdown decision
//! in `Orchestrator::all_stages_terminal`.
//!
//! Split out of `recovery.rs`'s `mod tests` to keep that file under the
//! maintainability limit — the same trick `recovery_sync_tests.rs` uses.
//!
//! Regression coverage for a daemon that shut itself down mid-plan: a
//! dispute's verdict moved a stage to `Queued` on disk and closed the judge
//! session (emptying `active_sessions`) before `sync_graph_with_stage_files`
//! had re-marked the graph node, so the old `all_stages_terminal` still saw
//! the node at `NeedsAdjudication` — a status it treated as terminal — and
//! shut the daemon down with the `Queued` stage left unspawned.

use super::*;
use tempfile::TempDir;

use crate::fs::work_dir::write_terminal_config;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::schema::{Implementers, StageDefinition, StageSandboxConfig};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::save_stage;

fn minimal_stage_definition(id: &str) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: None,
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_ceiling_tokens: None,
        removed_context_budget: None,
        plan_overview: None,
        sandbox: StageSandboxConfig::default(),
        execution_mode: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        code_review: None,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    }
}

fn pending_retry_failure() -> FailureInfo {
    FailureInfo {
        failure_type: FailureType::SessionCrash,
        detected_at: chrono::Utc::now(),
        evidence: vec!["crashed".to_string()],
    }
}

fn stage_with(status: StageStatus) -> Stage {
    Stage {
        id: "t".to_string(),
        status,
        ..Stage::default()
    }
}

#[test]
fn always_terminal_statuses() {
    for status in [
        StageStatus::Completed,
        StageStatus::Skipped,
        StageStatus::MergeConflict,
        StageStatus::CompletedWithFailures,
        StageStatus::MergeBlocked,
        StageStatus::NeedsHumanReview,
    ] {
        assert!(
            stage_file_is_terminal(&stage_with(status.clone())),
            "{status:?} must be terminal"
        );
    }
}

#[test]
fn never_terminal_statuses() {
    for status in [
        StageStatus::NeedsAdjudication,
        StageStatus::Executing,
        StageStatus::WaitingForInput,
        StageStatus::NeedsHandoff,
    ] {
        assert!(
            !stage_file_is_terminal(&stage_with(status.clone())),
            "{status:?} must never be terminal"
        );
    }
}

#[test]
fn blocked_and_held_depend_on_retry_and_hold() {
    let blocked_no_retry = stage_with(StageStatus::Blocked);
    assert!(
        stage_file_is_terminal(&blocked_no_retry),
        "a Blocked stage with no pending retry must be terminal"
    );

    let mut blocked_pending_retry = stage_with(StageStatus::Blocked);
    blocked_pending_retry.failure_info = Some(pending_retry_failure());
    assert!(
        !stage_file_is_terminal(&blocked_pending_retry),
        "a Blocked stage with a pending retry must not be terminal"
    );

    let mut queued_held = stage_with(StageStatus::Queued);
    queued_held.held = true;
    assert!(
        stage_file_is_terminal(&queued_held),
        "a held Queued stage must be terminal"
    );

    let queued_unheld = stage_with(StageStatus::Queued);
    assert!(
        !stage_file_is_terminal(&queued_unheld),
        "an unheld Queued stage must not be terminal"
    );

    let mut waiting_held = stage_with(StageStatus::WaitingForDeps);
    waiting_held.held = true;
    assert!(
        stage_file_is_terminal(&waiting_held),
        "a held WaitingForDeps stage must be terminal"
    );

    let waiting_unheld = stage_with(StageStatus::WaitingForDeps);
    assert!(
        !stage_file_is_terminal(&waiting_unheld),
        "an unheld WaitingForDeps stage must not be terminal"
    );
}

/// Build a single-node `Orchestrator` where the graph node sits at
/// `graph_status` while the on-disk stage file carries `file_stage` — the
/// exact shape of the one-tick lag `all_stages_terminal` must resolve from
/// the file, not the graph.
fn orchestrator_with_one_node(
    graph_status: StageStatus,
    file_stage: Stage,
) -> (Orchestrator, TempDir) {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().to_path_buf();
    write_terminal_config(
        &work_dir,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    save_stage(&file_stage, &work_dir).unwrap();

    let mut graph = ExecutionGraph::build(vec![minimal_stage_definition(&file_stage.id)]).unwrap();
    graph.force_status(&file_stage.id, graph_status).unwrap();

    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let orchestrator = Orchestrator::new(config, graph).unwrap();
    (orchestrator, temp)
}

#[test]
fn graph_lag_needs_adjudication_over_queued_file_is_not_terminal() {
    let stage = Stage {
        id: "alpha".to_string(),
        status: StageStatus::Queued,
        ..Stage::default()
    };
    let (orchestrator, _temp) = orchestrator_with_one_node(StageStatus::NeedsAdjudication, stage);
    assert!(!orchestrator.all_stages_terminal());
}

#[test]
fn needs_adjudication_file_is_never_terminal() {
    let stage = Stage {
        id: "alpha".to_string(),
        status: StageStatus::NeedsAdjudication,
        ..Stage::default()
    };
    let (orchestrator, _temp) = orchestrator_with_one_node(StageStatus::NeedsAdjudication, stage);
    assert!(!orchestrator.all_stages_terminal());
}

#[test]
fn needs_human_review_file_is_terminal() {
    let stage = Stage {
        id: "alpha".to_string(),
        status: StageStatus::NeedsHumanReview,
        ..Stage::default()
    };
    let (orchestrator, _temp) = orchestrator_with_one_node(StageStatus::NeedsHumanReview, stage);
    assert!(orchestrator.all_stages_terminal());
}
