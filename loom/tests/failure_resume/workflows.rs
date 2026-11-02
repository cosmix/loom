//! Dependency, preservation, and recovery workflows.
use super::*;

#[test]
fn test_multiple_blocked_stages_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage_a = create_test_stage("stage-a", "Stage A", StageStatus::Blocked);
    save_stage(&stage_a, work_dir).unwrap();

    let mut stage_b = create_test_stage("stage-b", "Stage B", StageStatus::Blocked);
    stage_b.add_dependency("stage-a".to_string());
    save_stage(&stage_b, work_dir).unwrap();

    let mut stage_c = create_test_stage("stage-c", "Stage C", StageStatus::WaitingForDeps);
    stage_c.add_dependency("stage-b".to_string());
    save_stage(&stage_c, work_dir).unwrap();

    let triggered = trigger_dependents("stage-a", work_dir, work_dir, "main").unwrap();
    assert!(triggered.is_empty());

    let triggered = trigger_dependents("stage-b", work_dir, work_dir, "main").unwrap();
    assert!(triggered.is_empty());

    // Blocked -> Queued (per state machine)
    transition_stage("stage-a", StageStatus::Queued, work_dir).unwrap();
    transition_stage("stage-a", StageStatus::Executing, work_dir).unwrap();
    transition_stage("stage-a", StageStatus::Completed, work_dir).unwrap();
    transition_stage("stage-a", StageStatus::Completed, work_dir).unwrap();

    let triggered = trigger_dependents("stage-a", work_dir, work_dir, "main").unwrap();
    assert!(triggered.is_empty());

    let stage_b = load_stage("stage-b", work_dir).unwrap();
    assert_eq!(stage_b.status, StageStatus::Blocked);
}

#[test]
fn test_blocked_stage_preserves_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new(
        "Complex Stage".to_string(),
        Some("A complex test stage".to_string()),
    );
    stage.id = "complex-stage".to_string();
    stage.status = StageStatus::Executing;
    stage.add_dependency("dep-1".to_string());
    stage.add_dependency("dep-2".to_string());
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple("Criterion A".to_string()));
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple("Criterion B".to_string()));
    stage.add_file_pattern("src/**/*.rs".to_string());
    stage.add_file_pattern("tests/**/*.rs".to_string());
    stage.parallel_group = Some("group-1".to_string());

    let created_at = stage.created_at;
    save_stage(&stage, work_dir).unwrap();

    let blocked = transition_stage("complex-stage", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked.status, StageStatus::Blocked);
    assert_eq!(blocked.dependencies.len(), 2);
    assert_eq!(blocked.acceptance.len(), 2);
    assert_eq!(blocked.files.len(), 2);
    assert_eq!(blocked.parallel_group, Some("group-1".to_string()));
    assert_eq!(
        blocked.description,
        Some("A complex test stage".to_string())
    );
    assert_eq!(blocked.created_at, created_at);
    assert!(blocked.updated_at > created_at);
}

#[test]
fn test_needs_handoff_preserves_stage_context() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let mut stage = Stage::new(
        "Handoff Stage".to_string(),
        Some("Needs handoff due to context exhaustion".to_string()),
    );
    stage.id = "handoff-stage".to_string();
    stage.status = StageStatus::Executing;
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(
        "Must complete integration".to_string(),
    ));
    stage.add_file_pattern("src/integration/**/*.rs".to_string());
    stage.set_worktree(Some("worktree-123".to_string()));
    stage.assign_session("session-abc".to_string());

    save_stage(&stage, work_dir).unwrap();

    let needs_handoff =
        transition_stage("handoff-stage", StageStatus::NeedsHandoff, work_dir).unwrap();
    assert_eq!(needs_handoff.status, StageStatus::NeedsHandoff);
    assert_eq!(needs_handoff.worktree, Some("worktree-123".to_string()));
    assert_eq!(needs_handoff.session, Some("session-abc".to_string()));
    assert_eq!(needs_handoff.acceptance.len(), 1);
    assert_eq!(needs_handoff.files.len(), 1);
}

#[test]
fn test_cascading_failure_does_not_propagate() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage_1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    save_stage(&stage_1, work_dir).unwrap();

    let mut stage_2 = create_test_stage("stage-2", "Stage 2", StageStatus::Executing);
    stage_2.add_dependency("stage-1".to_string());
    save_stage(&stage_2, work_dir).unwrap();

    let mut stage_3 = create_test_stage("stage-3", "Stage 3", StageStatus::WaitingForDeps);
    stage_3.add_dependency("stage-2".to_string());
    save_stage(&stage_3, work_dir).unwrap();

    let blocked_stage_2 = transition_stage("stage-2", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked_stage_2.status, StageStatus::Blocked);

    let triggered = trigger_dependents("stage-2", work_dir, work_dir, "main").unwrap();
    assert!(triggered.is_empty());

    let stage_3 = load_stage("stage-3", work_dir).unwrap();
    assert_eq!(stage_3.status, StageStatus::WaitingForDeps);
}

#[test]
fn test_recovery_workflow_blocked_to_completion() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    let stage = create_test_stage("recovery-stage", "Recovery Test", StageStatus::Executing);
    save_stage(&stage, work_dir).unwrap();

    let blocked = transition_stage("recovery-stage", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked.status, StageStatus::Blocked);

    // Blocked -> Queued (per state machine, Blocked can only go to Queued)
    let ready = transition_stage("recovery-stage", StageStatus::Queued, work_dir).unwrap();
    assert_eq!(ready.status, StageStatus::Queued);

    let executing = transition_stage("recovery-stage", StageStatus::Executing, work_dir).unwrap();
    assert_eq!(executing.status, StageStatus::Executing);

    let completed = transition_stage("recovery-stage", StageStatus::Completed, work_dir).unwrap();
    assert_eq!(completed.status, StageStatus::Completed);
    assert!(completed.completed_at.is_some());
}

#[test]
fn test_parallel_stages_one_blocked_one_succeeds() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    fs::create_dir_all(work_dir.join("stages")).unwrap();

    // Stage 1 must be Completed AND merged for dependents to be triggered
    let mut stage_1 = create_test_stage("stage-1", "Stage 1", StageStatus::Completed);
    stage_1.merged = true;
    save_stage(&stage_1, work_dir).unwrap();

    let mut stage_2a = create_test_stage("stage-2a", "Stage 2A", StageStatus::WaitingForDeps);
    stage_2a.add_dependency("stage-1".to_string());
    save_stage(&stage_2a, work_dir).unwrap();

    let mut stage_2b = create_test_stage("stage-2b", "Stage 2B", StageStatus::WaitingForDeps);
    stage_2b.add_dependency("stage-1".to_string());
    save_stage(&stage_2b, work_dir).unwrap();

    let mut stage_3 = create_test_stage("stage-3", "Stage 3", StageStatus::WaitingForDeps);
    stage_3.add_dependency("stage-2a".to_string());
    stage_3.add_dependency("stage-2b".to_string());
    save_stage(&stage_3, work_dir).unwrap();

    let triggered = trigger_dependents("stage-1", work_dir, work_dir, "main").unwrap();
    assert_eq!(triggered.len(), 2);

    transition_stage("stage-2a", StageStatus::Executing, work_dir).unwrap();
    let blocked_2a = transition_stage("stage-2a", StageStatus::Blocked, work_dir).unwrap();
    assert_eq!(blocked_2a.status, StageStatus::Blocked);

    transition_stage("stage-2b", StageStatus::Executing, work_dir).unwrap();
    transition_stage("stage-2b", StageStatus::Completed, work_dir).unwrap();
    transition_stage("stage-2b", StageStatus::Completed, work_dir).unwrap();

    let triggered = trigger_dependents("stage-2b", work_dir, work_dir, "main").unwrap();
    assert!(triggered.is_empty());

    let stage_3 = load_stage("stage-3", work_dir).unwrap();
    assert_eq!(stage_3.status, StageStatus::WaitingForDeps);
}
