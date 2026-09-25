//! Core orchestrator for coordinating stage execution
//!
//! The orchestrator is the heart of `loom run`. It:
//! - Creates worktrees for ready stages
//! - Spawns Claude sessions in terminal windows
//! - Monitors stage completion and session health
//! - Handles crashes and context exhaustion
//! - Manages the execution graph

use std::io::{self, Write};

mod coherence;
mod completion_handler;
mod contract_budget;
mod crash_classification;
mod crash_handler;
mod event_handler;
mod heartbeat_apply;
mod inbox_drain;
mod judge_close;
mod merge_handler;
mod orchestrator;
mod orphan_adoption;
mod persistence;
mod recovery;
mod recovery_guards;
mod recovery_queued_sync;
mod run;
mod run_result;
mod session_adoption;
mod session_lifecycle;
mod spawn_setup;
mod spool_drain;
mod stage_executor;
mod stage_handoff;
mod stage_spawn;
mod stage_telemetry;
pub mod state_identity;
mod verdict_apply;

pub(crate) use crash_classification::spawn_failure_type;
pub use orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};
pub use state_identity::{abort_foreign_state, check_lock_identity, LockCheck, LockIdentity};

/// Clear the current line (status line) before printing a message.
/// This prevents output from being mangled when the status line is being updated.
pub(super) fn clear_status_line() {
    // \r moves cursor to start of line, \x1B[K clears from cursor to end of line
    print!("\r\x1B[K");
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::frontmatter::extract_yaml_frontmatter;
    use crate::plan::schema::{SandboxConfig, StageDefinition};
    use crate::plan::ExecutionGraph;
    use std::path::PathBuf;
    use std::time::Duration;

    fn create_test_config() -> OrchestratorConfig {
        OrchestratorConfig {
            max_parallel_sessions: 2,
            poll_interval: Duration::from_millis(100),
            manual_mode: true,
            watch_mode: false,
            work_dir: PathBuf::from("/tmp/test-work"),
            repo_root: PathBuf::from("/tmp/test-repo"),
            status_update_interval: Duration::from_secs(30),
            auto_merge: false,
            base_branch: None,
            skills_dir: None,
            enable_skill_routing: false, // Disable for tests
            max_skill_recommendations: 8,
            sandbox_config: SandboxConfig::default(),
            shutdown_flag: None,
            lock_identity: None,
            plan_id: None,
        }
    }

    fn create_simple_graph() -> ExecutionGraph {
        let stages = vec![StageDefinition {
            id: "stage-1".to_string(),
            name: "Stage 1".to_string(),
            working_dir: ".".to_string(),
            ..Default::default()
        }];

        ExecutionGraph::build(stages).unwrap()
    }

    #[test]
    fn test_orchestrator_config_default() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.max_parallel_sessions, 4);
        assert_eq!(config.poll_interval, Duration::from_secs(5));
        assert!(!config.manual_mode);
        assert!(!config.watch_mode);
        assert!(config.auto_merge);
    }

    #[test]
    fn test_orchestrator_result_success() {
        let result = OrchestratorResult {
            completed_stages: vec!["stage-1".to_string()],
            failed_stages: vec![],
            needs_handoff: vec![],
            total_sessions_spawned: 1,
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
        };

        assert!(result.is_success());
    }

    #[test]
    fn test_orchestrator_result_failure() {
        let result = OrchestratorResult {
            completed_stages: vec![],
            failed_stages: vec!["stage-1".to_string()],
            needs_handoff: vec![],
            total_sessions_spawned: 1,
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
        };

        assert!(!result.is_success());
    }

    #[test]
    fn test_orchestrator_result_needs_handoff() {
        let result = OrchestratorResult {
            completed_stages: vec![],
            failed_stages: vec![],
            needs_handoff: vec!["stage-1".to_string()],
            total_sessions_spawned: 1,
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
        };

        assert!(!result.is_success());
    }

    /// Acceptance: a knowledge-stage spawn no longer writes the main repo's
    /// `.claude/settings.local.json` — the capsule built at spawn carries the
    /// resolved sandbox settings into the session directly.
    #[test]
    fn validate_knowledge_sandbox_writes_nothing_to_local_settings() {
        use crate::fs::work_dir::write_terminal_config;
        use crate::models::session::{SessionBackendKind, TerminalConfig};
        use crate::models::stage::Stage;

        let temp = tempfile::TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        let work = repo_root.join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        // Pin the terminal backend to tmux so `Orchestrator::new` never runs
        // real terminal detection, which fails on a headless test runner
        // (same trick `stage_executor_tests.rs::work_dir` uses).
        write_terminal_config(
            &work,
            &TerminalConfig {
                backend: SessionBackendKind::Tmux,
            },
        )
        .unwrap();

        let config = OrchestratorConfig {
            work_dir: work.clone(),
            repo_root: repo_root.clone(),
            enable_skill_routing: false,
            ..Default::default()
        };
        let mut orchestrator =
            Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();

        let stage = Stage::new("knowledge stage".to_string(), None);
        let result = orchestrator
            .validate_knowledge_sandbox(&stage, "knowledge-1")
            .unwrap();

        assert!(result.is_some(), "a default sandbox config must validate");
        assert!(
            !repo_root.join(".claude/settings.local.json").exists(),
            "knowledge-stage spawn setup must not write the main repo's local settings"
        );
    }

    /// Acceptance: a knowledge-stage spawn with an invalid sandbox config
    /// still blocks with `SandboxSetupFailure`, not a propagated error that
    /// would kill the daemon while the stage sits Executing with no session.
    #[test]
    fn validate_knowledge_sandbox_blocks_an_invalid_config() {
        use crate::fs::work_dir::write_terminal_config;
        use crate::models::failure::FailureType;
        use crate::models::session::{SessionBackendKind, TerminalConfig};
        use crate::models::stage::{Stage, StageStatus};
        use crate::verify::transitions::{load_stage, save_stage};

        let temp = tempfile::TempDir::new().unwrap();
        let repo_root = temp.path().join("repo");
        let work = repo_root.join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        write_terminal_config(
            &work,
            &TerminalConfig {
                backend: SessionBackendKind::Tmux,
            },
        )
        .unwrap();

        let config = OrchestratorConfig {
            work_dir: work.clone(),
            repo_root: repo_root.clone(),
            enable_skill_routing: false,
            ..Default::default()
        };
        let mut orchestrator =
            Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();

        let mut stage = Stage::new("knowledge stage".to_string(), None);
        stage.id = "knowledge-1".to_string();
        stage.status = StageStatus::Executing;
        stage.sandbox.enabled = Some(false);
        save_stage(&stage, &work).unwrap();

        let result = orchestrator
            .validate_knowledge_sandbox(&stage, "knowledge-1")
            .unwrap();
        assert!(
            result.is_none(),
            "an invalid sandbox config must refuse to spawn"
        );

        let after = load_stage("knowledge-1", &work).unwrap();
        assert_eq!(after.status, StageStatus::Blocked);
        assert_eq!(
            after.failure_info.map(|f| f.failure_type),
            Some(FailureType::SandboxSetupFailure)
        );
    }

    #[test]
    fn install_required_hooks_rejects_missing_hooks_dir() {
        let error = stage_executor::install_required_hooks(None, "missing-hooks")
            .expect_err("missing hooks directory must abort spawn setup");

        assert!(error.to_string().contains("hooks directory not found"));
    }

    #[test]
    fn install_required_hooks_accepts_a_present_hooks_dir_without_writing_anything() {
        let hooks_dir = tempfile::tempdir().unwrap();

        stage_executor::install_required_hooks(Some(hooks_dir.path().to_path_buf()), "stage-1")
            .expect("a present hooks directory must not abort spawn setup");
    }

    #[test]
    #[ignore] // Requires a terminal emulator - skipped in CI
    fn test_running_session_count() {
        let config = create_test_config();
        let graph = create_simple_graph();
        let orchestrator = Orchestrator::new(config, graph).expect("Failed to create orchestrator");

        assert_eq!(orchestrator.running_session_count(), 0);
    }

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
id: stage-1
name: Test Stage
status: Pending
---

# Stage Details
Test content
"#;

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.get("id").is_some());
        assert!(value.get("name").is_some());
    }

    #[test]
    fn test_extract_yaml_frontmatter_no_delimiter() {
        let content = "No frontmatter here";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_yaml_frontmatter_not_closed() {
        let content = r#"---
id: stage-1
name: Test Stage
"#;
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
    }
}
