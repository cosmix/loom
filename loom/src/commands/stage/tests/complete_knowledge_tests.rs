use super::*;

/// A knowledge session completing with an untracked file in the knowledge
/// directory is told to commit first, and relays nothing.
#[test]
#[serial]
fn a_knowledge_session_with_uncommitted_knowledge_is_told_to_commit_first() {
    let temp = setup_work_dir();
    initialize_zero_commit_repo(temp.path());
    let work_dir = temp.path().join(".loom/work");
    let mut stage = create_test_stage("test-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir, &stage);
    let knowledge = temp.path().join("doc/loom/knowledge");
    std::fs::create_dir_all(&knowledge).unwrap();
    std::fs::write(knowledge.join("uncommitted.md"), "# Draft\n").unwrap();
    let scratch_root = TempDir::new().unwrap();
    let scratch = scratch_root.path().join("session-1");
    std::fs::create_dir(&scratch).unwrap();
    let private = <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700);
    std::fs::set_permissions(&scratch, private).unwrap();

    let context = relay_context(scratch.clone(), "knowledge", &work_dir);
    let call = session_call(&work_dir, temp.path(), false);
    let (code, text) = routed(RelayMode::Relay(context), &call);

    assert_eq!(code, Some(1), "{text}");
    assert!(text.starts_with("Host job: refused (complete, "), "{text}");
    assert!(
        text.contains("Commit first: completion verifies committed content only."),
        "{text}"
    );
    assert_eq!(std::fs::read_dir(&scratch).unwrap().count(), 0, "no ticket");
}

#[test]
#[serial]
fn human_review_force_completes_knowledge_without_a_branch() {
    let temp = setup_work_dir();
    initialize_zero_commit_repo(temp.path());
    let work_dir = temp.path().join(".loom/work");
    let mut stage = create_test_stage("knowledge-bootstrap", StageStatus::NeedsHumanReview);
    stage.stage_type = StageType::Knowledge;
    stage.review_reason = Some("Acceptance cannot run in the stage environment".into());
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".into())];
    save_test_stage(&work_dir, &stage);
    let mut dependent = create_test_stage("implementation", StageStatus::WaitingForDeps);
    dependent.dependencies = vec![stage.id.clone()];
    save_test_stage(&work_dir, &dependent);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp.path()).unwrap();
    let result = crate::commands::stage::human_review(stage.id.clone(), false, true, None);
    assert!(result.is_ok(), "force completion failed: {result:?}");

    let completed = load_stage(&stage.id, &work_dir).unwrap();
    assert_eq!(completed.status, StageStatus::Completed);
    assert!(completed.merged);
    assert!(completed.review_reason.is_none());
    assert!(completed.completed_at.is_some());
    assert!(completed.completed_commit.is_none());
    assert_eq!(
        load_stage(&dependent.id, &work_dir).unwrap().status,
        StageStatus::Queued
    );
}

#[test]
#[serial]
fn test_complete_knowledge_stage_sets_merged_true() {
    let temp_dir = setup_work_dir();
    initialize_zero_commit_repo(temp_dir.path());
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage (no acceptance criteria)
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete_stage("knowledge-stage", false, None);

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
    // Key assertion: merged=true is auto-set for knowledge stages
    assert!(
        loaded_stage.merged,
        "Knowledge stage should auto-set merged=true"
    );
}

#[test]
#[serial]
fn test_complete_knowledge_stage_with_passing_acceptance() {
    let temp_dir = setup_work_dir();
    initialize_zero_commit_repo(temp_dir.path());
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage with passing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete_stage("knowledge-stage", false, None);

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_stage.status, StageStatus::Completed);
    assert!(loaded_stage.merged);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_with_failing_acceptance() {
    let temp_dir = setup_work_dir();
    initialize_zero_commit_repo(temp_dir.path());
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage with failing acceptance criteria
    let mut stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    stage.stage_type = StageType::Knowledge;
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 1".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete_stage("knowledge-stage", false, None);

    // New behavior: acceptance failure returns Err and stage stays Executing
    assert!(
        result.is_err(),
        "complete() should return Err when acceptance fails"
    );

    let loaded_stage = load_stage("knowledge-stage", &work_dir_path).unwrap();
    // Stage should remain Executing (not transition to CompletedWithFailures)
    assert_eq!(loaded_stage.status, StageStatus::Executing);
    // merged should NOT be set when acceptance fails
    assert!(!loaded_stage.merged);
}

#[test]
#[serial]
fn test_complete_knowledge_stage_triggers_dependents() {
    let temp_dir = setup_work_dir();
    initialize_zero_commit_repo(temp_dir.path());
    let work_dir_path = temp_dir.path().join(".loom").join("work");

    // Create a knowledge stage
    let mut knowledge_stage = create_test_stage("knowledge-stage", StageStatus::Executing);
    knowledge_stage.stage_type = StageType::Knowledge;
    save_test_stage(&work_dir_path, &knowledge_stage);

    // Create a dependent stage waiting for the knowledge stage
    let mut dependent_stage = create_test_stage("dependent-stage", StageStatus::WaitingForDeps);
    dependent_stage.dependencies = vec!["knowledge-stage".to_string()];
    save_test_stage(&work_dir_path, &dependent_stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete_stage("knowledge-stage", false, None);

    assert!(result.is_ok(), "complete() failed: {:?}", result.err());

    // Verify knowledge stage is completed with merged=true
    let loaded_knowledge = load_stage("knowledge-stage", &work_dir_path).unwrap();
    assert_eq!(loaded_knowledge.status, StageStatus::Completed);
    assert!(loaded_knowledge.merged);

    // Verify dependent stage was triggered to Queued
    let loaded_dependent = load_stage("dependent-stage", &work_dir_path).unwrap();
    assert_eq!(
        loaded_dependent.status,
        StageStatus::Queued,
        "Dependent stage should be triggered to Queued when knowledge stage completes with merged=true"
    );
}
