//! Unit tests for the pure recovery guards in `recovery_guards.rs`.
//!
//! Split out of `recovery.rs`'s `mod tests` to keep that file under the
//! maintainability limit — the same trick `recovery_adoption_tests.rs` and
//! `recovery_sync_tests.rs` use.

use super::*;
use tempfile::TempDir;

use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{SessionBackendKind, TerminalConfig};
use crate::models::stage::StageType;
use crate::orchestrator::core::OrchestratorConfig;
use crate::plan::schema::{Implementers, StageDefinition, StageSandboxConfig};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::save_stage;

fn stage_definition(id: &str, dependencies: Vec<String>) -> StageDefinition {
    StageDefinition {
        id: id.to_string(),
        name: id.to_string(),
        description: None,
        dependencies,
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
        skills: vec![],
    }
}

// ----- plan_mismatch -----

fn stage_with_plan(id: &str, plan_id: Option<&str>) -> Stage {
    let mut stage = Stage::new(id.to_string(), None);
    stage.id = id.to_string();
    stage.plan_id = plan_id.map(str::to_string);
    stage
}

#[test]
fn plan_mismatch_is_none_when_neither_side_has_an_opinion() {
    assert!(recovery_guards::plan_mismatch(None, &stage_with_plan("alpha", None)).is_none());
}

#[test]
fn plan_mismatch_is_none_when_the_file_has_no_plan_id() {
    assert!(
        recovery_guards::plan_mismatch(Some("plan-a"), &stage_with_plan("alpha", None)).is_none()
    );
}

#[test]
fn plan_mismatch_is_none_when_ids_agree() {
    assert!(recovery_guards::plan_mismatch(
        Some("plan-a"),
        &stage_with_plan("alpha", Some("plan-a"))
    )
    .is_none());
}

#[test]
fn plan_mismatch_names_both_ids_and_the_stage_on_disagreement() {
    let reason =
        recovery_guards::plan_mismatch(Some("plan-a"), &stage_with_plan("alpha", Some("plan-b")))
            .expect("differing plan ids must be reported");
    assert!(
        reason.contains("plan-a"),
        "reason must name the daemon's plan: {reason}"
    );
    assert!(
        reason.contains("plan-b"),
        "reason must name the file's plan: {reason}"
    );
    assert!(
        reason.contains("alpha"),
        "reason must name the stage: {reason}"
    );
}

// ----- too_young_to_judge -----

#[test]
fn a_session_created_a_second_ago_is_too_young_to_judge() {
    let now = chrono::Utc::now();
    let mut session = Session::new();
    session.created_at = now - chrono::Duration::seconds(1);
    assert!(recovery_guards::too_young_to_judge(&session, now));
}

#[test]
fn a_session_created_five_minutes_ago_is_not_too_young() {
    let now = chrono::Utc::now();
    let mut session = Session::new();
    session.created_at = now - chrono::Duration::minutes(5);
    assert!(!recovery_guards::too_young_to_judge(&session, now));
}

// ----- queued_writeback_verdict (pure function, real stage files on disk) -----

/// `alpha`'s dependency check is exempt from git ancestry by using a
/// `Knowledge`-typed dependency (see `are_all_dependencies_satisfied` in
/// `verify::transitions::state`, which only requires `Completed` + `merged`
/// for a knowledge stage) — this keeps the guard test a pure file-state
/// check with no git repo to set up.
fn save_knowledge_dependency(work_dir: &std::path::Path, status: StageStatus, merged: bool) {
    let mut alpha = Stage::new("alpha".to_string(), None);
    alpha.id = "alpha".to_string();
    alpha.stage_type = StageType::Knowledge;
    alpha.status = status;
    alpha.merged = merged;
    save_stage(&alpha, work_dir).unwrap();
}

fn beta_depending_on_alpha() -> Stage {
    let mut beta = Stage::new("beta".to_string(), None);
    beta.id = "beta".to_string();
    beta.dependencies = vec!["alpha".to_string()];
    beta.status = StageStatus::WaitingForDeps;
    beta
}

#[test]
fn queued_writeback_verdict_writes_when_the_dependency_is_completed_and_merged() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    save_knowledge_dependency(&work_dir, StageStatus::Completed, true);
    let beta = beta_depending_on_alpha();

    match recovery_guards::queued_writeback_verdict(&beta, &work_dir, temp.path(), "main") {
        recovery_guards::QueuedWriteback::Write => {}
        _ => panic!("a Completed + merged knowledge dependency must yield Write"),
    }
}

#[test]
fn queued_writeback_verdict_reports_dependencies_unmet_when_the_dependency_is_still_waiting() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    save_knowledge_dependency(&work_dir, StageStatus::WaitingForDeps, false);
    let beta = beta_depending_on_alpha();

    match recovery_guards::queued_writeback_verdict(&beta, &work_dir, temp.path(), "main") {
        recovery_guards::QueuedWriteback::DependenciesUnmet => {}
        _ => panic!("a WaitingForDeps dependency must yield DependenciesUnmet"),
    }
}

// ----- Orchestrator-level: sync_queued_status_to_files must not trust a
// stale graph over the stage file's own unmet dependencies -----

/// Builds the stale-graph fixture: the GRAPH's own edges have already
/// promoted `beta` to `Queued` (via `mark_merged("alpha")`), while `alpha`'s
/// stage FILE stays `WaitingForDeps` — the state a recreated `.loom/work`
/// can leave behind under a running daemon.
fn stale_graph_promotes_beta_fixture() -> (TempDir, Orchestrator) {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().to_path_buf();
    write_terminal_config(
        &work_dir,
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();

    // alpha's FILE stays WaitingForDeps — never completed/merged on disk.
    save_knowledge_dependency(&work_dir, StageStatus::WaitingForDeps, false);
    let beta = beta_depending_on_alpha();
    save_stage(&beta, &work_dir).unwrap();

    let mut graph = ExecutionGraph::build(vec![
        stage_definition("alpha", vec![]),
        stage_definition("beta", vec!["alpha".to_string()]),
    ])
    .unwrap();
    // Drive the GRAPH's alpha node to Completed + merged independent of the
    // file, then let the graph's own readiness logic promote beta.
    graph.mark_queued("alpha").unwrap();
    graph.mark_executing("alpha").unwrap();
    graph.mark_completed("alpha").unwrap();
    graph.mark_merged("alpha").unwrap();
    assert_eq!(
        graph.get_node("beta").unwrap().status,
        StageStatus::Queued,
        "test setup must reproduce the graph considering beta ready"
    );

    let config = OrchestratorConfig {
        work_dir: work_dir.clone(),
        repo_root: temp.path().to_path_buf(),
        enable_skill_routing: false,
        ..Default::default()
    };
    let orchestrator = Orchestrator::new(config, graph).unwrap();
    (temp, orchestrator)
}

/// Regression test for the recovery bug this module exists to close:
/// `sync_queued_status_to_files` must leave `beta`'s file `WaitingForDeps`
/// rather than trust a graph that considers it ready.
#[test]
fn sync_queued_status_to_files_does_not_write_queued_over_unmet_file_dependencies() {
    let (temp, mut orchestrator) = stale_graph_promotes_beta_fixture();
    let work_dir = temp.path().to_path_buf();

    orchestrator
        .sync_queued_status_to_files()
        .expect("sync must not error when a queued-writeback dependency check fails closed");

    assert_eq!(
        crate::verify::transitions::load_stage("beta", &work_dir)
            .unwrap()
            .status,
        StageStatus::WaitingForDeps,
        "beta's file must stay WaitingForDeps when its own dependency file is unmet, \
         even though the graph considers it ready"
    );
}
