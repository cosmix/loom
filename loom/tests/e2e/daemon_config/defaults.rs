//! Tests for default and custom orchestrator configuration values

use loom::orchestrator::terminal::BackendType;
use loom::orchestrator::OrchestratorConfig;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_orchestrator_config_default_values() {
    let config = OrchestratorConfig::default();

    assert_eq!(config.max_parallel_sessions, 4);
    assert_eq!(config.poll_interval, Duration::from_secs(5));
    assert!(!config.manual_mode);
    assert!(!config.watch_mode);
    assert!(!config.auto_merge);
    assert_eq!(config.status_update_interval, Duration::from_secs(30));
    assert_eq!(config.backend_type, BackendType::Native);
}

#[test]
fn test_orchestrator_config_custom_values() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();

    let config = OrchestratorConfig {
        max_parallel_sessions: 8,
        poll_interval: Duration::from_secs(10),
        manual_mode: true,
        watch_mode: true,
        work_dir: work_dir.to_path_buf(),
        repo_root: work_dir.to_path_buf(),
        status_update_interval: Duration::from_secs(60),
        backend_type: BackendType::Native,
        auto_merge: true,
        base_branch: None,
    };

    assert_eq!(config.max_parallel_sessions, 8);
    assert_eq!(config.poll_interval, Duration::from_secs(10));
    assert!(config.manual_mode);
    assert!(config.watch_mode);
    assert!(config.auto_merge);
    assert_eq!(config.status_update_interval, Duration::from_secs(60));
}
