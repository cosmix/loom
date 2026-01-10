//! Integration tests for context variable expansion in acceptance criteria
//!
//! These tests verify that context variables like `${WORKTREE}`, `${PROJECT_ROOT}`,
//! and `${STAGE_ID}` are properly expanded when running acceptance criteria.

use crate::helpers::create_temp_git_repo;
use loom::models::stage::Stage;
use loom::verify::context::CriteriaContext;
use loom::verify::criteria::run_acceptance;
use std::fs;
use tempfile::tempdir;

/// Test that ${WORKTREE} variable is available and expands correctly
#[test]
fn test_worktree_variable_expansion() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let worktree_path = temp_dir.path();

    let ctx = CriteriaContext::new(worktree_path);

    assert!(
        ctx.get_variable("WORKTREE").is_some(),
        "WORKTREE variable should be set"
    );
    assert_eq!(
        ctx.get_variable("WORKTREE").unwrap(),
        worktree_path.display().to_string()
    );

    // Test expansion in a command string
    let expanded = ctx.expand("cd ${WORKTREE} && pwd");
    assert!(
        expanded.contains(&worktree_path.display().to_string()),
        "WORKTREE should be expanded in command"
    );
}

/// Test that ${PROJECT_ROOT} is detected when Cargo.toml exists
#[test]
fn test_project_root_detection_cargo() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let worktree_path = temp_dir.path();

    // Create Cargo.toml to trigger PROJECT_ROOT detection
    fs::write(
        worktree_path.join("Cargo.toml"),
        "[package]\nname = \"test\"",
    )
    .expect("failed to write Cargo.toml");

    let ctx = CriteriaContext::new(worktree_path);

    assert!(
        ctx.get_variable("PROJECT_ROOT").is_some(),
        "PROJECT_ROOT should be detected with Cargo.toml"
    );
    assert_eq!(
        ctx.get_variable("PROJECT_ROOT").unwrap(),
        worktree_path.display().to_string()
    );
}

/// Test that ${PROJECT_ROOT} is detected when package.json exists
#[test]
fn test_project_root_detection_npm() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let worktree_path = temp_dir.path();

    // Create package.json to trigger PROJECT_ROOT detection
    fs::write(worktree_path.join("package.json"), "{\"name\": \"test\"}")
        .expect("failed to write package.json");

    let ctx = CriteriaContext::new(worktree_path);

    assert!(
        ctx.get_variable("PROJECT_ROOT").is_some(),
        "PROJECT_ROOT should be detected with package.json"
    );
}

/// Test that ${PROJECT_ROOT} is detected in subdirectory
#[test]
fn test_project_root_detection_in_subdirectory() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let worktree_path = temp_dir.path();

    // Create subdirectory with Cargo.toml
    let subdir = worktree_path.join("my-project");
    fs::create_dir(&subdir).expect("failed to create subdir");
    fs::write(subdir.join("Cargo.toml"), "[package]").expect("failed to write Cargo.toml");

    let ctx = CriteriaContext::new(worktree_path);

    assert!(
        ctx.get_variable("PROJECT_ROOT").is_some(),
        "PROJECT_ROOT should be detected in subdirectory"
    );
    assert!(
        ctx.get_variable("PROJECT_ROOT")
            .unwrap()
            .contains("my-project"),
        "PROJECT_ROOT should point to subdirectory with manifest"
    );
}

/// Test that ${PROJECT_ROOT} is not set when no manifest exists
#[test]
fn test_project_root_not_set_without_manifest() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let worktree_path = temp_dir.path();

    // No manifest files created
    let ctx = CriteriaContext::new(worktree_path);

    assert!(
        ctx.get_variable("PROJECT_ROOT").is_none(),
        "PROJECT_ROOT should not be set without manifest"
    );
}

/// Test that ${STAGE_ID} is available with with_stage_id constructor
#[test]
fn test_stage_id_variable() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let worktree_path = temp_dir.path();

    let ctx = CriteriaContext::with_stage_id(worktree_path, "my-test-stage");

    assert_eq!(
        ctx.get_variable("STAGE_ID"),
        Some("my-test-stage"),
        "STAGE_ID should match provided value"
    );

    let expanded = ctx.expand("echo ${STAGE_ID}");
    assert_eq!(expanded, "echo my-test-stage");
}

/// Test that unknown variables remain unexpanded
#[test]
fn test_unknown_variable_unchanged() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let ctx = CriteriaContext::new(temp_dir.path());

    let expanded = ctx.expand("echo ${UNKNOWN_VARIABLE}");
    assert_eq!(
        expanded, "echo ${UNKNOWN_VARIABLE}",
        "Unknown variables should remain unchanged"
    );
}

/// Test find_unresolved identifies missing variables
#[test]
fn test_find_unresolved_variables() {
    let ctx = CriteriaContext::default();

    let unresolved = ctx.find_unresolved("cd ${PROJECT_ROOT} && ${CUSTOM_VAR}/run.sh");
    assert_eq!(unresolved.len(), 2);
    assert!(unresolved.contains(&"PROJECT_ROOT".to_string()));
    assert!(unresolved.contains(&"CUSTOM_VAR".to_string()));
}

/// Test that acceptance criteria with variables execute correctly
#[test]
fn test_acceptance_criteria_with_worktree_variable() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    let mut stage = Stage::new("test-stage".to_string(), None);
    stage.id = "test-stage-123".to_string();

    if cfg!(target_family = "unix") {
        // Test that ${WORKTREE} expands to the working directory
        stage.add_acceptance_criterion("test -d \"${WORKTREE}\"".to_string());
    } else {
        stage.add_acceptance_criterion("if exist \"${WORKTREE}\" (exit /b 0)".to_string());
    }

    let result = run_acceptance(&stage, Some(temp_dir.path())).expect("should execute criteria");
    assert!(
        result.all_passed(),
        "Criteria with WORKTREE variable should pass"
    );
}

/// Test that acceptance criteria with PROJECT_ROOT execute correctly
#[test]
fn test_acceptance_criteria_with_project_root_variable() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    // Create Cargo.toml so PROJECT_ROOT is detected
    fs::write(temp_dir.path().join("Cargo.toml"), "[package]").expect("failed to write Cargo.toml");

    let mut stage = Stage::new("test-stage".to_string(), None);
    stage.id = "test-stage-456".to_string();

    if cfg!(target_family = "unix") {
        stage.add_acceptance_criterion("test -f \"${PROJECT_ROOT}/Cargo.toml\"".to_string());
    } else {
        stage.add_acceptance_criterion(
            "if exist \"${PROJECT_ROOT}\\Cargo.toml\" (exit /b 0)".to_string(),
        );
    }

    let result = run_acceptance(&stage, Some(temp_dir.path())).expect("should execute criteria");
    assert!(
        result.all_passed(),
        "Criteria with PROJECT_ROOT variable should pass"
    );
}

/// Test that setup commands also have variables expanded
#[test]
fn test_setup_commands_with_variables() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    let mut stage = Stage::new("test-stage".to_string(), None);
    stage.id = "setup-test".to_string();

    if cfg!(target_family = "unix") {
        // Setup uses ${WORKTREE} variable
        stage.setup.push("cd ${WORKTREE}".to_string());
        stage.add_acceptance_criterion("pwd".to_string());
    } else {
        stage.setup.push("cd ${WORKTREE}".to_string());
        stage.add_acceptance_criterion("cd".to_string());
    }

    let result = run_acceptance(&stage, Some(temp_dir.path())).expect("should execute criteria");
    assert!(
        result.all_passed(),
        "Setup commands with variables should work"
    );
}

/// Test combining multiple variables in a single command
#[test]
fn test_multiple_variables_in_command() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    // Create manifest so PROJECT_ROOT is set
    fs::write(temp_dir.path().join("Cargo.toml"), "[package]").expect("failed to write Cargo.toml");

    let ctx = CriteriaContext::with_stage_id(temp_dir.path(), "multi-var-stage");

    let expanded = ctx.expand("cd ${PROJECT_ROOT} && echo ${STAGE_ID} > ${WORKTREE}/output.txt");

    assert!(
        !expanded.contains("${PROJECT_ROOT}"),
        "PROJECT_ROOT should be expanded"
    );
    assert!(
        !expanded.contains("${STAGE_ID}"),
        "STAGE_ID should be expanded"
    );
    assert!(
        !expanded.contains("${WORKTREE}"),
        "WORKTREE should be expanded"
    );
    assert!(
        expanded.contains("multi-var-stage"),
        "Stage ID should appear in expanded command"
    );
}

/// Test custom variables can be added
#[test]
fn test_custom_variable() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    let mut ctx = CriteriaContext::new(temp_dir.path());
    ctx.set_variable("MY_CUSTOM_VAR", "/custom/path");

    let expanded = ctx.expand("echo ${MY_CUSTOM_VAR}");
    assert_eq!(expanded, "echo /custom/path");
}

/// Test available_variables lists all set variables
#[test]
fn test_available_variables() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    // Create manifest so PROJECT_ROOT is set
    fs::write(temp_dir.path().join("Cargo.toml"), "[package]").expect("failed to write Cargo.toml");

    let ctx = CriteriaContext::with_stage_id(temp_dir.path(), "test-stage");

    let vars = ctx.available_variables();
    assert!(vars.contains(&"WORKTREE"), "Should have WORKTREE");
    assert!(vars.contains(&"PROJECT_ROOT"), "Should have PROJECT_ROOT");
    assert!(vars.contains(&"STAGE_ID"), "Should have STAGE_ID");
}

/// Test that variable expansion handles edge cases
#[test]
fn test_variable_expansion_edge_cases() {
    let ctx = CriteriaContext::default();

    // Empty variable name
    let expanded = ctx.expand("echo ${}");
    assert_eq!(
        expanded, "echo ${}",
        "Empty variable should remain unchanged"
    );

    // Nested braces (should not be supported)
    let expanded = ctx.expand("echo ${${VAR}}");
    assert_eq!(
        expanded, "echo ${${VAR}}",
        "Nested variables should remain unchanged"
    );

    // No closing brace
    let expanded = ctx.expand("echo ${UNCLOSED");
    assert_eq!(
        expanded, "echo ${UNCLOSED",
        "Unclosed variable should remain unchanged"
    );
}

/// Integration test: Run acceptance criteria in a git worktree context
#[test]
fn test_acceptance_in_git_worktree_context() {
    let repo = create_temp_git_repo().expect("failed to create git repo");

    // Create Cargo.toml so it looks like a Rust project
    fs::write(
        repo.path().join("Cargo.toml"),
        "[package]\nname = \"test-project\"",
    )
    .expect("failed to write Cargo.toml");

    let mut stage = Stage::new("Worktree Test".to_string(), None);
    stage.id = "worktree-test".to_string();

    if cfg!(target_family = "unix") {
        // Verify we're in a git repo and can access project root
        stage.add_acceptance_criterion("test -d \"${WORKTREE}/.git\"".to_string());
        stage.add_acceptance_criterion("test -f \"${PROJECT_ROOT}/Cargo.toml\"".to_string());
    } else {
        stage.add_acceptance_criterion("if exist \"${WORKTREE}\\.git\" (exit /b 0)".to_string());
        stage.add_acceptance_criterion(
            "if exist \"${PROJECT_ROOT}\\Cargo.toml\" (exit /b 0)".to_string(),
        );
    }

    let result = run_acceptance(&stage, Some(repo.path())).expect("should execute criteria");
    assert!(
        result.all_passed(),
        "All acceptance criteria should pass: {:?}",
        result.failures()
    );
}

/// Test that variable expansion works with complex shell commands
#[test]
fn test_variables_in_complex_shell_commands() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    // Create a test file
    fs::write(temp_dir.path().join("test.txt"), "hello world").expect("failed to write test file");

    let mut stage = Stage::new("Complex Commands".to_string(), None);
    stage.id = "complex-test".to_string();

    if cfg!(target_family = "unix") {
        // Chain of commands using variables
        stage.add_acceptance_criterion(
            "cd ${WORKTREE} && cat test.txt | grep -q 'hello'".to_string(),
        );
    } else {
        stage.add_acceptance_criterion("type \"${WORKTREE}\\test.txt\"".to_string());
    }

    let result = run_acceptance(&stage, Some(temp_dir.path())).expect("should execute criteria");
    assert!(
        result.all_passed(),
        "Complex shell commands with variables should work"
    );
}
