//! Tests for loom init command.

use super::cleanup::{cleanup_work_directory, prune_stale_worktrees};
use super::plan_setup::{create_stage_from_definition, initialize_with_plan};
use crate::fs::work_dir::WorkDir;
use crate::models::session::SessionBackendKind;
use crate::models::stage::{
    Implementer, Implementers, Stage, StageStatus, StageType as ModelStageType,
};
use crate::plan::schema::{
    AcceptanceCriterion, LoomConfig, LoomMetadata, SandboxConfig, StageDefinition,
    StageSandboxConfig, StageType,
};
use crate::verify::serialize_stage_to_markdown;
use chrono::Utc;
use serial_test::serial;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a minimal valid plan file
fn create_test_plan(dir: &Path, stages: Vec<StageDefinition>) -> PathBuf {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            auto_merge: None,
            sandbox: SandboxConfig::default(),
            change_impact: None,
            adjudication: None,
            stages,
        },
    };

    let yaml = serde_yaml::to_string(&metadata).unwrap();
    let plan_content = format!(
        "# Test Plan\n\n## Overview\n\nTest plan for unit tests\n\n<!-- loom METADATA -->\n```yaml\n{yaml}```\n<!-- END loom METADATA -->\n"
    );

    let plan_path = dir.join("test-plan.md");
    fs::write(&plan_path, plan_content).unwrap();
    plan_path
}

#[test]
fn test_create_stage_from_definition_no_dependencies() {
    let stage_def = StageDefinition {
        id: "stage-1".to_string(),
        name: "Stage 1".to_string(),
        description: Some("Test stage".to_string()),
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        setup: vec![],
        files: vec!["src/*.rs".to_string()],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_budget: None,
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
    };

    let stage = create_stage_from_definition(&stage_def, "plan-001");

    assert_eq!(stage.id, "stage-1");
    assert_eq!(stage.name, "Stage 1");
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.plan_id, Some("plan-001".to_string()));
    assert_eq!(stage.dependencies.len(), 0);
    assert_eq!(stage.acceptance.len(), 1);
    assert!(!stage.ultracode, "ultracode defaults to false");

    // The ultracode license propagates from the definition to the stage model
    let ultracode_def = StageDefinition {
        ultracode: true,
        ..stage_def.clone()
    };
    let ultracode_stage = create_stage_from_definition(&ultracode_def, "plan-001");
    assert!(ultracode_stage.ultracode);

    // The implementer lanes propagate from the definition to the stage model,
    // preserving preference ORDER — the first lane is what routine work reaches
    // for, so a reordering here would silently retarget every stage's subagents.
    assert_eq!(
        stage.implementers.preferred(),
        Implementer::Claude,
        "implementers defaults to the Claude lane"
    );
    assert!(!stage.implementers.includes_codex());

    let mixed_def = StageDefinition {
        implementers: Implementers::new(vec![Implementer::Codex, Implementer::Claude]),
        ..stage_def.clone()
    };
    let mixed_stage = create_stage_from_definition(&mixed_def, "plan-001");
    assert!(
        mixed_stage.implementers.is_mixed(),
        "a mixed lane list must survive definition → stage"
    );
    assert_eq!(mixed_stage.implementers.preferred(), Implementer::Codex);
    assert!(mixed_stage.implementers.includes_claude());

    // The subagent response budget propagates the same way. Omitted stays None
    // so the built-in default applies; an explicit value reaches the runtime
    // Stage, which is what the orchestrator measures the session against.
    assert_eq!(
        stage.subagent_timeout_secs, None,
        "subagent_timeout_secs defaults to None"
    );
    let budgeted_def = StageDefinition {
        subagent_timeout_secs: Some(1800),
        ..stage_def
    };
    let budgeted_stage = create_stage_from_definition(&budgeted_def, "plan-001");
    assert_eq!(budgeted_stage.subagent_timeout_secs, Some(1800));
}

#[test]
fn test_create_stage_from_definition_with_dependencies() {
    let stage_def = StageDefinition {
        id: "stage-2".to_string(),
        name: "Stage 2".to_string(),
        description: None,
        dependencies: vec!["stage-1".to_string()],
        parallel_group: Some("core".to_string()),
        acceptance: vec![],
        setup: vec!["cargo build".to_string()],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_budget: None,
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
    };

    let stage = create_stage_from_definition(&stage_def, "plan-002");

    assert_eq!(stage.id, "stage-2");
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert_eq!(stage.dependencies, vec!["stage-1".to_string()]);
    assert_eq!(stage.parallel_group, Some("core".to_string()));
}

#[test]
fn test_serialize_stage_to_markdown_minimal() {
    let stage = Stage {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        description: None,
        status: StageStatus::Queued,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        stage_type: ModelStageType::default(),
        context_budget: None,
        plan_id: None,
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
        started_at: None,
        duration_secs: None,
        execution_secs: None,
        attempt_started_at: None,
        close_reason: None,
        auto_merge: None,
        working_dir: Some(".".to_string()),
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
        base_branch: None,
        base_merged_from: vec![],
        outputs: vec![],
        completed_commit: None,
        merged: false,
        merge_conflict: false,
        verification_status: Default::default(),
        artifacts: Vec::new(),
        wiring: Vec::new(),
        wiring_tests: Vec::new(),
        dead_code_check: None,
        before_stage: Vec::new(),
        after_stage: Vec::new(),
        fix_attempts: 0,
        dispute_count: 0,
        evidence_rounds: 0,
        amendments_applied: 0,
        sandbox: Default::default(),
        execution_mode: None,
        max_fix_attempts: None,
        review_reason: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        is_possibly_stuck: false,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    };

    let content = serialize_stage_to_markdown(&stage).unwrap();

    assert!(content.starts_with("---\n"));
    assert!(content.contains("# Stage: Test Stage"));
    assert!(content.contains("**Status**: Queued"));
}

#[test]
fn test_serialize_stage_to_markdown_with_all_fields() {
    let stage = Stage {
        id: "full-stage".to_string(),
        name: "Full Stage".to_string(),
        description: Some("Detailed description".to_string()),
        status: StageStatus::Executing,
        dependencies: vec!["dep1".to_string(), "dep2".to_string()],
        parallel_group: Some("group1".to_string()),
        acceptance: vec![
            AcceptanceCriterion::Simple("test1".to_string()),
            AcceptanceCriterion::Simple("test2".to_string()),
        ],
        setup: vec![],
        files: vec!["file1.rs".to_string(), "file2.rs".to_string()],
        stage_type: ModelStageType::default(),
        context_budget: None,
        plan_id: Some("plan-123".to_string()),
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
        started_at: None,
        duration_secs: None,
        execution_secs: None,
        attempt_started_at: None,
        close_reason: None,
        auto_merge: None,
        working_dir: Some(".".to_string()),
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
        base_branch: None,
        base_merged_from: vec![],
        outputs: vec![],
        completed_commit: None,
        merged: false,
        merge_conflict: false,
        verification_status: Default::default(),
        artifacts: Vec::new(),
        wiring: Vec::new(),
        wiring_tests: Vec::new(),
        dead_code_check: None,
        before_stage: Vec::new(),
        after_stage: Vec::new(),
        fix_attempts: 0,
        dispute_count: 0,
        evidence_rounds: 0,
        amendments_applied: 0,
        sandbox: Default::default(),
        execution_mode: None,
        max_fix_attempts: None,
        review_reason: None,
        bug_fix: None,
        regression_test: None,
        model: None,
        reasoning_effort: None,
        is_possibly_stuck: false,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    };

    let content = serialize_stage_to_markdown(&stage).unwrap();

    assert!(content.contains("## Dependencies"));
    assert!(content.contains("- dep1"));
    assert!(content.contains("- dep2"));
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("- [ ] test1"));
    assert!(content.contains("- [ ] test2"));
    assert!(content.contains("## Files"));
    assert!(content.contains("- `file1.rs`"));
    assert!(content.contains("- `file2.rs`"));
}

#[test]
fn test_initialize_with_plan_nonexistent_file() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let nonexistent_path = temp_dir.path().join("nonexistent.md");

    let result = initialize_with_plan(&work_dir, &nonexistent_path, SessionBackendKind::Native);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("does not exist"));
}

#[test]
#[serial]
fn test_initialize_with_plan_creates_config() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let stage_def = StageDefinition {
        id: "test-stage".to_string(),
        name: "Test Stage".to_string(),
        description: None,
        dependencies: vec![],
        parallel_group: None,
        acceptance: vec![AcceptanceCriterion::Simple("echo ok".to_string())],
        setup: vec![],
        files: vec![],
        auto_merge: None,
        working_dir: ".".to_string(),
        stage_type: StageType::default(),
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        context_budget: None,
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
    };

    let plan_path = create_test_plan(temp_dir.path(), vec![stage_def]);

    let result = initialize_with_plan(&work_dir, &plan_path, SessionBackendKind::Native);

    assert!(result.is_ok());

    let config_path = work_dir.root().join("config.toml");
    assert!(config_path.exists());

    let config_content = fs::read_to_string(config_path).unwrap();
    assert!(config_content.contains("source_path"));
    assert!(config_content.contains("plan_id"));
}

#[test]
#[serial]
fn test_initialize_with_plan_creates_stage_files() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let stages = vec![
        StageDefinition {
            id: "stage-1".to_string(),
            name: "Stage One".to_string(),
            description: Some("First stage".to_string()),
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
            artifacts: vec![],
            wiring: vec![],
            wiring_tests: vec![],
            dead_code_check: None,
            before_stage: vec![],
            after_stage: vec![],
            context_budget: None,
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
        },
        StageDefinition {
            id: "stage-2".to_string(),
            name: "Stage Two".to_string(),
            description: None,
            dependencies: vec!["stage-1".to_string()],
            parallel_group: None,
            acceptance: vec![AcceptanceCriterion::Simple("echo ok".to_string())],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: StageType::default(),
            artifacts: vec![],
            wiring: vec![],
            wiring_tests: vec![],
            dead_code_check: None,
            before_stage: vec![],
            after_stage: vec![],
            context_budget: None,
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
        },
    ];

    let plan_path = create_test_plan(temp_dir.path(), stages);

    let result = initialize_with_plan(&work_dir, &plan_path, SessionBackendKind::Native);

    assert!(result.is_ok());

    let stages_dir = work_dir.root().join("stages");
    assert!(stages_dir.exists());

    let stage_files: Vec<_> = fs::read_dir(stages_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();

    assert_eq!(stage_files.len(), 2);
}

#[test]
fn test_cleanup_work_directory_removes_existing() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");

    fs::create_dir_all(&work_dir).unwrap();
    fs::write(work_dir.join("test.txt"), "content").unwrap();

    assert!(work_dir.exists());

    let result = cleanup_work_directory(temp_dir.path());

    assert!(result.is_ok());
    assert!(!work_dir.exists());
}

#[test]
fn test_cleanup_work_directory_nonexistent_ok() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");

    assert!(!work_dir.exists());

    let result = cleanup_work_directory(temp_dir.path());

    assert!(result.is_ok());
}

#[test]
fn test_initialize_with_plan_invalid_yaml() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp_dir.path()).unwrap();
    work_dir.initialize().unwrap();

    let invalid_plan = temp_dir.path().join("invalid.md");
    fs::write(
        &invalid_plan,
        "# Invalid Plan\n\n<!-- loom METADATA -->\n```yaml\ninvalid: yaml: content:\n```\n",
    )
    .unwrap();

    let result = initialize_with_plan(&work_dir, &invalid_plan, SessionBackendKind::Native);

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("parse"));
}

#[test]
fn test_prune_stale_worktrees_does_not_fail() {
    let temp_dir = TempDir::new().unwrap();

    let result = prune_stale_worktrees(temp_dir.path());

    assert!(result.is_ok());
}

#[test]
fn test_cleanup_orphaned_sessions_does_not_fail() {
    use super::cleanup::{cleanup_orphaned_sessions, SessionReapMode};

    let temp_dir = TempDir::new().unwrap();

    let result = cleanup_orphaned_sessions(temp_dir.path(), SessionReapMode::OrphansOnly);

    assert!(result.is_ok());
}

/// A "socket" here is any file dropped in the tmux socket directory named
/// `loom-<session-id>` — `list_loom_sockets` does not shell out to tmux, it
/// just reads directory entries, so a plain empty file is enough to exercise
/// attribution and reap-mode logic without a real tmux server. `kill_socket_server`
/// still shells out to a real (possibly absent) `tmux` binary, but treats any
/// failure as best-effort, so the fake socket file is always removed by
/// `cleanup_orphaned_sessions` regardless of whether tmux is installed here.
#[test]
#[serial]
fn test_cleanup_orphaned_sessions_reaps_live_only_in_clean_mode() {
    use super::cleanup::{cleanup_orphaned_sessions, SessionReapMode};
    use crate::fs::session_files::session_to_markdown;
    use crate::models::session::Session;

    // Points `TMUX_TMPDIR` at an isolated directory for the duration of the
    // test and restores it on drop, mirroring
    // `orchestrator::terminal::tmux::socket`'s own test guard. That guard is
    // `pub(crate)`, but its parent module (`tmux::socket`) is private and
    // re-exports only the socket API, so the guard is unreachable from here —
    // hence a copy rather than an import. Widening `mod socket` for a
    // test-only helper would be the wrong trade.
    struct TmuxTmpDirGuard {
        original: Option<std::ffi::OsString>,
    }
    impl TmuxTmpDirGuard {
        fn set(dir: &Path) -> Self {
            let original = std::env::var_os("TMUX_TMPDIR");
            std::env::set_var("TMUX_TMPDIR", dir);
            Self { original }
        }
    }
    impl Drop for TmuxTmpDirGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(value) => std::env::set_var("TMUX_TMPDIR", value),
                None => std::env::remove_var("TMUX_TMPDIR"),
            }
        }
    }

    let tmux_tmpdir = TempDir::new().unwrap();
    let _guard = TmuxTmpDirGuard::set(tmux_tmpdir.path());
    // Mirrors `orchestrator::terminal::tmux::socket::loom_socket_dir()`:
    // `$TMUX_TMPDIR/tmux-<uid>`.
    let uid = unsafe { libc::getuid() };
    let socket_dir = tmux_tmpdir.path().join(format!("tmux-{uid}"));
    fs::create_dir_all(&socket_dir).unwrap();

    let repo_root = TempDir::new().unwrap();
    let sessions_dir = repo_root.path().join(".work").join("sessions");
    fs::create_dir_all(&sessions_dir).unwrap();

    // Attributed + LIVE session: pid is this test process, which is
    // guaranteed to be alive for the duration of the test.
    let mut live_session = Session::new();
    live_session.id = "session-livecase00".to_string();
    live_session.pid = Some(std::process::id());
    fs::write(
        sessions_dir.join(format!("{}.md", live_session.id)),
        session_to_markdown(&live_session),
    )
    .unwrap();
    let live_socket = socket_dir.join(format!("loom-{}", live_session.id));
    fs::write(&live_socket, "").unwrap();

    // Unattributed socket: no session file anywhere claims this id.
    let unattributed_socket = socket_dir.join("loom-session-strangercase0");
    fs::write(&unattributed_socket, "").unwrap();

    // Normal mode: a live attributed session must survive; the unattributed
    // socket is always left alone.
    cleanup_orphaned_sessions(repo_root.path(), SessionReapMode::OrphansOnly).unwrap();
    assert!(
        live_socket.exists(),
        "a live attributed session must never be reaped outside --clean"
    );
    assert!(
        unattributed_socket.exists(),
        "an unattributed socket must never be touched"
    );

    // Clean mode: the live session is reaped because `.work/` (and thus
    // attribution) is about to be destroyed; the unattributed socket is
    // still left alone.
    cleanup_orphaned_sessions(repo_root.path(), SessionReapMode::IncludeLiveBeforeClean).unwrap();
    assert!(
        !live_socket.exists(),
        "clean mode must reap a live session before attribution is destroyed"
    );
    assert!(
        unattributed_socket.exists(),
        "an unattributed socket must never be touched, even in clean mode"
    );
}

#[test]
fn test_remove_work_directory_on_failure_removes_directory() {
    use super::cleanup::remove_work_directory_on_failure;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");

    fs::create_dir_all(&work_dir).unwrap();
    fs::write(work_dir.join("test.txt"), "content").unwrap();

    assert!(work_dir.exists());

    remove_work_directory_on_failure(temp_dir.path());

    assert!(!work_dir.exists());
}

#[test]
fn test_remove_work_directory_on_failure_nonexistent_ok() {
    use super::cleanup::remove_work_directory_on_failure;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");

    assert!(!work_dir.exists());

    remove_work_directory_on_failure(temp_dir.path());

    assert!(!work_dir.exists());
}
