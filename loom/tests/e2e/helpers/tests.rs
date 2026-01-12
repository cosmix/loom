//! Internal tests for helper functions

use super::*;
use loom::models::session::{Session, SessionStatus};
use loom::models::stage::{Stage, StageStatus};
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn test_create_temp_git_repo() {
    let repo = create_temp_git_repo().expect("Should create git repo");

    let git_dir = repo.path().join(".git");
    assert!(git_dir.exists(), "Git directory should exist");

    let readme = repo.path().join("README.md");
    assert!(readme.exists(), "README should exist");
}

#[test]
fn test_init_loom_with_plan() {
    let temp = TempDir::new().expect("Should create temp dir");
    let plan_content = r#"# Test Plan

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: stage-1
      name: "Test Stage"
```

<!-- END loom METADATA -->
"#;

    let plan_path = init_loom_with_plan(temp.path(), plan_content).expect("Should init loom");

    assert!(plan_path.exists(), "Plan file should exist");

    let work_dir = temp.path().join(".work");
    assert!(work_dir.exists(), ".work directory should exist");
    assert!(
        work_dir.join("stages").exists(),
        "stages directory should exist"
    );
    assert!(
        work_dir.join("sessions").exists(),
        "sessions directory should exist"
    );
}

#[test]
fn test_create_and_read_stage_file() {
    let temp = TempDir::new().expect("Should create temp dir");
    let work_dir = temp.path();

    let mut stage = Stage::new(
        "Test Stage".to_string(),
        Some("Test description".to_string()),
    );
    stage.id = "test-stage-1".to_string();
    stage.status = StageStatus::Queued;
    stage.add_dependency("dep-1".to_string());
    stage.add_acceptance_criterion("Tests pass".to_string());

    create_stage_file(work_dir, &stage).expect("Should create stage file");

    let loaded = read_stage_file(work_dir, "test-stage-1").expect("Should read stage file");

    assert_eq!(loaded.id, stage.id);
    assert_eq!(loaded.name, stage.name);
    assert_eq!(loaded.description, stage.description);
    assert_eq!(loaded.status, stage.status);
    assert_eq!(loaded.dependencies, stage.dependencies);
    assert_eq!(loaded.acceptance, stage.acceptance);
}

#[test]
fn test_create_and_read_session_file() {
    let temp = TempDir::new().expect("Should create temp dir");
    let work_dir = temp.path();

    let mut session = Session::new();
    session.id = "test-session-1".to_string();
    session.status = SessionStatus::Running;
    session.assign_to_stage("stage-1".to_string());

    create_session_file(work_dir, &session).expect("Should create session file");

    let loaded = read_session_file(work_dir, "test-session-1").expect("Should read session file");

    assert_eq!(loaded.id, session.id);
    assert_eq!(loaded.status, session.status);
    assert_eq!(loaded.stage_id, session.stage_id);
}

#[test]
fn test_create_signal_file() {
    let temp = TempDir::new().expect("Should create temp dir");
    let work_dir = temp.path();

    let signal_content = "# Signal: test-signal\n\nTest signal content";

    create_signal_file(work_dir, "test-signal", signal_content).expect("Should create signal file");

    let signal_path = work_dir
        .join(".work")
        .join("signals")
        .join("test-signal.md");
    assert!(signal_path.exists(), "Signal file should exist");

    let content = std::fs::read_to_string(signal_path).expect("Should read signal file");
    assert_eq!(content, signal_content);
}

#[test]
fn test_wait_for_condition_success() {
    use std::sync::{Arc, Mutex};

    let counter = Arc::new(Mutex::new(0));
    let counter_clone = counter.clone();

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        *counter_clone.lock().unwrap() = 1;
    });

    let result = wait_for_condition(
        || {
            let count = *counter.lock().unwrap();
            count == 1
        },
        1000,
    );

    assert!(result.is_ok(), "Condition should be met");
}

#[test]
fn test_wait_for_condition_timeout() {
    let result = wait_for_condition(|| false, 500);

    assert!(result.is_err(), "Should timeout");
    assert!(
        result.unwrap_err().to_string().contains("Timeout"),
        "Error should mention timeout"
    );
}

#[test]
fn test_extract_yaml_frontmatter() {
    let content = r#"---
id: test-1
name: Test
status: Pending
---

# Body content"#;

    let yaml = extract_yaml_frontmatter(content).expect("Should extract frontmatter");
    assert!(yaml.is_mapping());

    let map = yaml.as_mapping().unwrap();
    assert_eq!(
        map.get(serde_yaml::Value::String("id".to_string()))
            .unwrap()
            .as_str()
            .unwrap(),
        "test-1"
    );
}

#[test]
fn test_extract_yaml_frontmatter_missing_delimiter() {
    let content = "id: test-1\nname: Test";

    let result = extract_yaml_frontmatter(content);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No frontmatter"));
}
