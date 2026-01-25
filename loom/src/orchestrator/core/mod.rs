//! Core orchestrator for coordinating stage execution
//!
//! The orchestrator is the heart of `loom run`. It:
//! - Creates worktrees for ready stages
//! - Spawns Claude sessions in terminal windows
//! - Monitors stage completion and session health
//! - Handles crashes and context exhaustion
//! - Manages the execution graph

use std::io::{self, Write};

mod completion_handler;
mod crash_handler;
mod event_handler;
mod merge_handler;
mod orchestrator;
mod persistence;
mod recovery;
mod stage_executor;

pub use orchestrator::{Orchestrator, OrchestratorConfig, OrchestratorResult};

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
    use crate::plan::schema::StageDefinition;
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
            backend_type: crate::orchestrator::terminal::BackendType::Native,
            auto_merge: false,
            base_branch: None,
            skills_dir: None,
            enable_skill_routing: false, // Disable for tests
            max_skill_recommendations: 5,
        }
    }

    fn create_simple_graph() -> ExecutionGraph {
        let stages = vec![StageDefinition {
            id: "stage-1".to_string(),
            name: "Stage 1".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: crate::plan::schema::StageType::default(),
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
            context_budget: None,
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
        };

        assert!(!result.is_success());
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
