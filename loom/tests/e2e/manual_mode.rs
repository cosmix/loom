//! E2E tests for manual mode execution (signals without session spawning)
//!
//! Manual mode creates signal files for Ready stages without spawning sessions.
//! This allows developers to manually attach to sessions and execute work.

use loom::models::session::Session;
use loom::models::stage::{Stage, StageStatus};
use loom::models::worktree::Worktree;
use loom::orchestrator::signals::{generate_signal, list_signals, read_signal};
use loom::verify::transitions::save_stage;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_work_dir_structure(base: &std::path::Path) -> std::path::PathBuf {
    let work_dir = base.join(".work");
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::create_dir_all(work_dir.join("signals")).unwrap();
    fs::create_dir_all(work_dir.join("sessions")).unwrap();
    work_dir
}

fn create_test_session(id: &str, stage_id: &str) -> Session {
    let mut session = Session::new();
    session.id = id.to_string();
    session.assign_to_stage(stage_id.to_string());
    session
}

fn create_test_stage(id: &str, name: &str, status: StageStatus) -> Stage {
    let mut stage = Stage::new(name.to_string(), Some(format!("Description for {name}")));
    stage.id = id.to_string();
    stage.status = status;
    stage
}

fn create_test_worktree(stage_id: &str) -> Worktree {
    Worktree::new(
        stage_id.to_string(),
        PathBuf::from(format!("/repo/.worktrees/{stage_id}")),
        format!("loom/{stage_id}"),
    )
}

#[test]
fn test_manual_mode_creates_signals() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-abc123", "stage-1");
    let mut stage = create_test_stage("stage-1", "First Stage", StageStatus::Queued);
    stage.add_acceptance_criterion("cargo test".to_string());
    stage.add_file_pattern("src/*.rs".to_string());

    save_stage(&stage, &work_dir).unwrap();

    let worktree = create_test_worktree("stage-1");

    let signal_path =
        generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    assert!(signal_path.exists());
    assert_eq!(
        signal_path,
        work_dir.join("signals").join("session-abc123.md")
    );

    let content = fs::read_to_string(&signal_path).unwrap();
    assert!(content.contains("# Signal: session-abc123"));
    assert!(content.contains("- **Session**: session-abc123"));
    assert!(content.contains("- **Stage**: stage-1"));
    assert!(content.contains("First Stage"));
}

#[test]
fn test_signal_file_format() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-test-456", "stage-2");
    let mut stage = create_test_stage("stage-2", "Second Stage", StageStatus::Queued);
    stage.description = Some("Implement the feature".to_string());
    stage.add_acceptance_criterion("cargo test passes".to_string());
    stage.add_acceptance_criterion("cargo clippy passes".to_string());
    stage.add_file_pattern("src/models/*.rs".to_string());
    stage.add_file_pattern("src/commands/*.rs".to_string());
    stage.set_plan("plan-test".to_string());

    let worktree = create_test_worktree("stage-2");

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let signal_content = read_signal("session-test-456", &work_dir)
        .unwrap()
        .expect("Signal should exist");

    assert_eq!(signal_content.session_id, "session-test-456");
    assert_eq!(signal_content.stage_id, "stage-2");
    assert_eq!(signal_content.plan_id, Some("plan-test".to_string()));
    assert_eq!(signal_content.stage_name, "Second Stage");
    assert!(signal_content.description.contains("Implement the feature"));

    assert_eq!(signal_content.acceptance_criteria.len(), 2);
    assert!(signal_content
        .acceptance_criteria
        .contains(&"cargo test passes".to_string()));
    assert!(signal_content
        .acceptance_criteria
        .contains(&"cargo clippy passes".to_string()));

    // Note: context_files is populated from ## Context Restoration section,
    // which is only present in fallback mode. New format embeds content directly.

    assert_eq!(signal_content.files_to_modify.len(), 2);
    assert!(signal_content
        .files_to_modify
        .contains(&"src/models/*.rs".to_string()));
    assert!(signal_content
        .files_to_modify
        .contains(&"src/commands/*.rs".to_string()));
}

#[test]
fn test_signal_not_created_for_pending_stages() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let stage1 = create_test_stage(
        "stage-pending",
        "Pending Stage",
        StageStatus::WaitingForDeps,
    );
    save_stage(&stage1, &work_dir).unwrap();

    let mut stage2 = create_test_stage("stage-ready", "Ready Stage", StageStatus::Queued);
    stage2.add_acceptance_criterion("Complete work".to_string());
    save_stage(&stage2, &work_dir).unwrap();

    let session_ready = create_test_session("session-ready", "stage-ready");
    let worktree_ready = create_test_worktree("stage-ready");
    generate_signal(
        &session_ready,
        &stage2,
        &worktree_ready,
        &[],
        None,
        None,
        &work_dir,
    )
    .unwrap();

    let signals = list_signals(&work_dir).unwrap();

    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0], "session-ready");
}

#[test]
fn test_manual_mode_creates_signal_without_session_spawn() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-manual-test", "stage-manual");
    let stage = create_test_stage("stage-manual", "Manual Stage", StageStatus::Queued);
    let worktree = create_test_worktree("stage-manual");

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let signal_path = work_dir.join("signals").join("session-manual-test.md");
    assert!(signal_path.exists());
    assert_eq!(stage.status, StageStatus::Queued);
}

#[test]
fn test_signal_includes_files_to_modify() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-context", "stage-context");
    let mut stage = create_test_stage("stage-context", "Context Stage", StageStatus::Queued);
    stage.add_file_pattern("src/orchestrator/*.rs".to_string());
    stage.add_file_pattern("src/models/stage.rs".to_string());

    let worktree = create_test_worktree("stage-context");

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let signal_path = work_dir.join("signals").join("session-context.md");
    let content = fs::read_to_string(&signal_path).unwrap();

    // Signal should include Files to Modify section with file patterns
    assert!(content.contains("## Files to Modify"));
    assert!(content.contains("src/orchestrator/*.rs"));
    assert!(content.contains("src/models/stage.rs"));
}

#[test]
fn test_signal_includes_embedded_handoff() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let handoffs_dir = work_dir.join("handoffs");
    fs::create_dir_all(&handoffs_dir).unwrap();

    let handoff_file = "2026-01-07-previous-work";
    let handoff_path = handoffs_dir.join(format!("{handoff_file}.md"));
    fs::write(&handoff_path, "# Previous work handoff content").unwrap();

    let session = create_test_session("session-with-handoff", "stage-with-handoff");
    let stage = create_test_stage(
        "stage-with-handoff",
        "Stage With Handoff",
        StageStatus::Queued,
    );
    let worktree = create_test_worktree("stage-with-handoff");

    generate_signal(
        &session,
        &stage,
        &worktree,
        &[],
        Some(handoff_file),
        None,
        &work_dir,
    )
    .unwrap();

    let signal_path = work_dir.join("signals").join("session-with-handoff.md");
    let content = fs::read_to_string(&signal_path).unwrap();

    // Handoff content should be embedded directly in the signal
    assert!(content.contains("## Previous Session Handoff"));
    assert!(content.contains("# Previous work handoff content"));
}

#[test]
fn test_signal_acceptance_criteria_format() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-acceptance", "stage-acceptance");
    let mut stage = create_test_stage("stage-acceptance", "Acceptance Test", StageStatus::Queued);
    stage.add_acceptance_criterion("All unit tests pass".to_string());
    stage.add_acceptance_criterion("Integration tests pass".to_string());
    stage.add_acceptance_criterion("No linting errors".to_string());

    let worktree = create_test_worktree("stage-acceptance");

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let signal_path = work_dir.join("signals").join("session-acceptance.md");
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("- [ ] All unit tests pass"));
    assert!(content.contains("- [ ] Integration tests pass"));
    assert!(content.contains("- [ ] No linting errors"));
}

#[test]
fn test_signal_with_dependencies_status() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-deps", "stage-with-deps");
    let mut stage = create_test_stage("stage-with-deps", "Stage With Deps", StageStatus::Queued);
    stage.add_dependency("stage-dep-1".to_string());
    stage.add_dependency("stage-dep-2".to_string());

    let worktree = create_test_worktree("stage-with-deps");

    use loom::orchestrator::signals::DependencyStatus;
    let deps = vec![
        DependencyStatus {
            stage_id: "stage-dep-1".to_string(),
            name: "First Dependency".to_string(),
            status: "Verified".to_string(),
            outputs: Vec::new(),
        },
        DependencyStatus {
            stage_id: "stage-dep-2".to_string(),
            name: "Second Dependency".to_string(),
            status: "Verified".to_string(),
            outputs: Vec::new(),
        },
    ];

    generate_signal(&session, &stage, &worktree, &deps, None, None, &work_dir).unwrap();

    let signal_path = work_dir.join("signals").join("session-deps.md");
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Dependencies Status"));
    assert!(content.contains("First Dependency"));
    assert!(content.contains("Second Dependency"));
    assert!(content.contains("Verified"));
}

#[test]
fn test_signal_default_tasks_when_none_provided() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-default-tasks", "stage-default-tasks");
    let stage = create_test_stage(
        "stage-default-tasks",
        "Stage Default Tasks",
        StageStatus::Queued,
    );
    let worktree = create_test_worktree("stage-default-tasks");

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let signal_path = work_dir.join("signals").join("session-default-tasks.md");
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Immediate Tasks"));
    assert!(content.contains("1. Review stage acceptance criteria above"));
    assert!(content.contains("2. Implement required changes"));
    assert!(content.contains("3. Verify all acceptance criteria are met"));
}

#[test]
fn test_multiple_signals_can_coexist() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session1 = create_test_session("session-1", "stage-1");
    let stage1 = create_test_stage("stage-1", "Stage 1", StageStatus::Queued);
    let worktree1 = create_test_worktree("stage-1");

    let session2 = create_test_session("session-2", "stage-2");
    let stage2 = create_test_stage("stage-2", "Stage 2", StageStatus::Queued);
    let worktree2 = create_test_worktree("stage-2");

    let session3 = create_test_session("session-3", "stage-3");
    let stage3 = create_test_stage("stage-3", "Stage 3", StageStatus::Queued);
    let worktree3 = create_test_worktree("stage-3");

    generate_signal(&session1, &stage1, &worktree1, &[], None, None, &work_dir).unwrap();
    generate_signal(&session2, &stage2, &worktree2, &[], None, None, &work_dir).unwrap();
    generate_signal(&session3, &stage3, &worktree3, &[], None, None, &work_dir).unwrap();

    let signals = list_signals(&work_dir).unwrap();

    assert_eq!(signals.len(), 3);
    assert!(signals.contains(&"session-1".to_string()));
    assert!(signals.contains(&"session-2".to_string()));
    assert!(signals.contains(&"session-3".to_string()));
}

#[test]
fn test_signal_worktree_information() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = create_work_dir_structure(temp_dir.path());

    let session = create_test_session("session-worktree-info", "stage-worktree-info");
    let stage = create_test_stage(
        "stage-worktree-info",
        "Worktree Info Stage",
        StageStatus::Queued,
    );
    let worktree = create_test_worktree("stage-worktree-info");

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let signal_path = work_dir.join("signals").join("session-worktree-info.md");
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Target"));
    assert!(content.contains("- **Worktree**:"));
    assert!(content.contains("- **Branch**: loom/stage-worktree-info"));
}
