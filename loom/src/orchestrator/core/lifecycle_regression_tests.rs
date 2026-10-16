//! Restart and full scheduling-pass regression fixtures.
use super::recovery::Recovery;
use super::stage_executor::{get_dependency_status, StageExecutor};
use super::{Orchestrator, OrchestratorConfig};
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::{Session, SessionBackendKind, TerminalConfig};
use crate::models::stage::{Stage, StageOutput, StageStatus, StageType};
use crate::plan::{schema::StageDefinition, ExecutionGraph};
use crate::verify::transitions::{load_stage, save_stage, update_stage};
use tempfile::TempDir;

fn fixture(stages: &[Stage]) -> (TempDir, Orchestrator) {
    let temp = TempDir::new().unwrap();
    crate::fs::work_dir::write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    for stage in stages {
        save_stage(stage, temp.path()).unwrap();
    }
    let graph = ExecutionGraph::build(
        stages
            .iter()
            .map(|stage| StageDefinition {
                id: stage.id.clone(),
                name: stage.id.clone(),
                dependencies: stage.dependencies.clone(),
                ..Default::default()
            })
            .collect(),
    )
    .unwrap();
    let orchestrator = Orchestrator::new(
        OrchestratorConfig {
            work_dir: temp.path().to_owned(),
            repo_root: temp.path().to_owned(),
            max_parallel_sessions: 1,
            enable_skill_routing: false,
            ..Default::default()
        },
        graph,
    )
    .unwrap();
    (temp, orchestrator)
}

fn stage(id: &str) -> Stage {
    Stage {
        id: id.into(),
        name: id.into(),
        status: StageStatus::Queued,
        ..Default::default()
    }
}

#[test]
fn scheduler_scans_past_all_held_candidates_and_target_held_knowledge() {
    for count in [1, 3] {
        let mut stages: Vec<_> = (0..count)
            .map(|i| {
                let mut stage = stage(&format!("a-{i}"));
                stage.held = true;
                stage
            })
            .collect();
        let mut knowledge = stage("a-knowledge");
        knowledge.stage_type = StageType::Knowledge;
        stages.extend([knowledge, stage("b")]);
        let (temp, mut orchestrator) = fixture(&stages);
        orchestrator.target_hold = Some(crate::git::target_guard::Hold {
            accepted: "old".into(),
            observed: "new".into(),
            reasons: vec![],
            since: chrono::Utc::now(),
        });
        let mut session = Session::new();
        session.assign_to_stage("b".into());
        crate::orchestrator::terminal::native::write_test_pid_identity(
            temp.path(),
            &session,
            std::process::id(),
        )
        .unwrap();
        crate::fs::session_files::save_session(&session, temp.path()).unwrap();
        assert_eq!(orchestrator.start_ready_stages().unwrap(), 1);
        assert!(orchestrator.active_sessions.contains_key("b"));
        assert_eq!(
            load_stage("b", temp.path()).unwrap().status,
            StageStatus::Executing
        );
        assert!(!orchestrator.spawn_blocks.contains_key("b"));
    }
}

#[test]
fn restart_retires_blocked_session_without_changing_intent_or_retry_budget() {
    for crash in [false, true] {
        let mut blocked = stage("a");
        blocked.status = StageStatus::Blocked;
        blocked.close_reason = Some(
            if crash {
                "crash exhausted"
            } else {
                "human review rejected"
            }
            .into(),
        );
        blocked.retry_count = 3;
        blocked.max_retries = Some(3);
        blocked.last_failure_at = Some(chrono::Utc::now());
        if crash {
            blocked.failure_info = Some(FailureInfo {
                failure_type: FailureType::SessionCrash,
                detected_at: chrono::Utc::now(),
                evidence: vec![],
            });
        }
        let mut session = Session::new();
        session.assign_to_stage("a".into());
        session.created_at = chrono::Utc::now() - chrono::Duration::minutes(10);
        blocked.session = Some(session.id.clone());
        let (temp, mut orchestrator) = fixture(&[blocked.clone()]);
        crate::fs::session_files::save_session(&session, temp.path()).unwrap();
        orchestrator.liveness =
            crate::orchestrator::liveness::LivenessService::fixed_for_tests(false);
        assert_eq!(orchestrator.recover_orphaned_sessions().unwrap(), 1);
        orchestrator.sync_graph_with_stage_files().unwrap();
        let after = load_stage("a", temp.path()).unwrap();
        assert_eq!(after.status, StageStatus::Blocked);
        assert_eq!(after.close_reason, blocked.close_reason);
        assert_eq!(after.retry_count, 3);
        assert_eq!(after.last_failure_at, blocked.last_failure_at);
        assert!(after.session.is_none());
    }
}

#[test]
fn corrupt_or_missing_stage_fails_snapshot_and_repair_restores_coverage() {
    let (temp, mut orchestrator) = fixture(&[stage("a"), stage("b")]);
    let path = crate::fs::stage_files::find_stage_file(&temp.path().join("stages"), "b")
        .unwrap()
        .unwrap();
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, "---\ninvalid: [\n---").unwrap();
    assert!(orchestrator.sync_graph_with_stage_files().is_err());
    assert!(orchestrator.start_ready_stages().is_err());
    assert!(!orchestrator.all_stages_terminal());
    std::fs::write(&path, original).unwrap();
    orchestrator.sync_graph_with_stage_files().unwrap();
    assert_eq!(
        orchestrator.stage_snapshot().unwrap().ready_ids(),
        vec!["a", "b"]
    );
    std::fs::remove_file(path).unwrap();
    assert!(orchestrator.stage_snapshot().is_err());
}

#[test]
fn removed_last_output_disappears_from_generated_downstream_brief() {
    let mut upstream = stage("a");
    upstream.status = StageStatus::Completed;
    upstream.merged = true;
    upstream.stage_type = StageType::Knowledge;
    upstream.outputs.push(StageOutput {
        key: "endpoint".into(),
        value: serde_json::json!("obsolete-endpoint"),
        description: "endpoint".into(),
    });
    let mut downstream = stage("b");
    downstream.dependencies.push("a".into());
    let (temp, mut orchestrator) = fixture(&[upstream, downstream.clone()]);
    orchestrator.sync_graph_with_stage_files().unwrap();
    let before = brief(&orchestrator, &downstream, temp.path());
    assert!(before.contains("obsolete-endpoint"));
    update_stage("a", temp.path(), |stage| {
        stage.outputs.clear();
        Ok(())
    })
    .unwrap();
    orchestrator.sync_graph_with_stage_files().unwrap();
    let after = brief(&orchestrator, &downstream, temp.path());
    assert!(!after.contains("obsolete-endpoint"));
}

fn brief(orchestrator: &Orchestrator, stage: &Stage, work: &std::path::Path) -> String {
    let deps = get_dependency_status(stage, &orchestrator.graph);
    let worktree = crate::models::worktree::Worktree::new(
        stage.id.clone(),
        work.to_path_buf(),
        format!("loom/{}", stage.id),
    );
    let path = crate::orchestrator::signals::generate_signal(
        &Session::new(),
        stage,
        &worktree,
        &deps,
        None,
        None,
        work,
    )
    .unwrap();
    std::fs::read_to_string(path).unwrap()
}
