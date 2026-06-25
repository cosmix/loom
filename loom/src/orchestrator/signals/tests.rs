use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::models::worktree::Worktree;
use crate::plan::schema::{AcceptanceCriterion, CodeReviewConfig};

use super::crud::{list_signals, read_signal, remove_signal, update_signal};
use super::format::{extract_tasks_from_description, format_signal_content};
use super::generate::{extract_plan_overview, generate_signal, render_review_dimensions};
use super::types::{DependencyStatus, EmbeddedContext, SignalUpdates};

// Submodules with additional tests
#[path = "tests_cache.rs"]
mod tests_cache;
#[path = "tests_merge.rs"]
mod tests_merge;
#[path = "tests_working_dir.rs"]
mod tests_working_dir;

// Public helpers for submodules
pub fn create_test_session() -> Session {
    let mut session = Session::new();
    session.id = "session-test-123".to_string();
    session.assign_to_stage("stage-1".to_string());
    session
}

pub fn create_test_stage() -> Stage {
    let mut stage = Stage::new(
        "Implement signals module".to_string(),
        Some("Create signal file generation logic".to_string()),
    );
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple(
        "Signal files are generated correctly".to_string(),
    ));
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple("All tests pass".to_string()));
    stage.add_file_pattern("src/orchestrator/signals.rs".to_string());
    stage
}

pub fn create_test_worktree() -> Worktree {
    Worktree::new(
        "stage-1".to_string(),
        PathBuf::from("/repo/.worktrees/stage-1"),
        "loom/stage-1".to_string(),
    )
}

#[test]
fn test_generate_signal_basic() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    let result = generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir);

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    assert!(signal_path.exists());

    let content = fs::read_to_string(&signal_path).unwrap();
    assert!(content.contains("# Signal: session-test-123"));
    assert!(content.contains("- **Session**: session-test-123"));
    assert!(content.contains("- **Stage**: stage-1"));
    assert!(content.contains("## Assignment"));
    assert!(content.contains("Implement signals module"));
}

/// End-to-end: a stage whose files are Rust sources must produce a signal that
/// directs the agent to load `loom-rust` via the Skill tool. This is the path
/// that was silently broken — `get_by_name("rust")` never matched `loom-rust`.
#[test]
fn test_signal_directs_agent_to_load_language_skill_from_files() {
    use crate::skills::SkillIndex;
    use std::io::Write;

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    // Mirror how skills are installed: ~/.claude/skills/loom-rust/SKILL.md.
    let skills_dir = temp_dir.path().join("skills");
    let rust_dir = skills_dir.join("loom-rust");
    fs::create_dir_all(&rust_dir).unwrap();
    let mut f = fs::File::create(rust_dir.join("SKILL.md")).unwrap();
    writeln!(f, "---").unwrap();
    writeln!(f, "name: loom-rust").unwrap();
    writeln!(f, "description: Rust language expertise for idiomatic code").unwrap();
    writeln!(f, "---").unwrap();
    let index = SkillIndex::load_from_directory(&skills_dir).unwrap();

    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.files = vec!["loom/src/**/*.rs".to_string()];
    let worktree = create_test_worktree();

    // No project-level languages passed: detection must come from stage.files.
    let signal_path = super::generate::generate_signal_with_skills(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &work_dir,
        Some(&index),
        &[],
    )
    .unwrap();

    let content = fs::read_to_string(&signal_path).unwrap();
    assert!(
        content.contains("Load these now"),
        "signal should carry a load-now directive:\n{content}"
    );
    assert!(
        content.contains("Skill(skill=\"loom-rust\")"),
        "signal should instruct invoking the loom-rust Skill:\n{content}"
    );
}

#[test]
fn test_generate_signal_with_dependencies() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    let deps = vec![DependencyStatus {
        stage_id: "stage-0".to_string(),
        name: "Setup models".to_string(),
        status: "completed".to_string(),
        outputs: Vec::new(),
    }];

    let result = generate_signal(&session, &stage, &worktree, &deps, None, None, &work_dir);

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Dependencies Status"));
    assert!(content.contains("Setup models"));
    assert!(content.contains("completed"));
}

#[test]
fn test_generate_signal_with_handoff() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    let result = generate_signal(
        &session,
        &stage,
        &worktree,
        &[],
        Some("2026-01-06-previous-work"),
        None,
        &work_dir,
    );

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Context Restoration"));
    assert!(content.contains("2026-01-06-previous-work.md"));
}

#[test]
fn test_format_signal_content() {
    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let content = format_signal_content(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    assert!(content.contains("# Signal: session-test-123"));
    assert!(content.contains("## Worktree Context"));
    assert!(content.contains("This signal contains everything you need"));
    assert!(content.contains("## Target"));
    assert!(content.contains("## Assignment"));
    assert!(content.contains("## Immediate Tasks"));
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("## Files to Modify"));
    assert!(content.contains("src/orchestrator/signals.rs"));
}

#[test]
fn test_format_signal_content_with_embedded_context() {
    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext {
        handoff_content: Some("# Handoff\nPrevious session completed tasks A and B.".to_string()),
        parsed_handoff: None,
        plan_overview: Some("# Plan Title\n\n## Overview\nThis plan does X.".to_string()),
        knowledge_has_content: false,
        memory_content: None,
        skill_recommendations: Vec::new(),
        context_budget: None,
        context_usage: None,
        sandbox_summary: None,
        cross_stage_summary: None,
        wiring_checklist: None,
        ultracode: false,
    };

    let content = format_signal_content(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Verify embedded content is present
    assert!(content.contains("## Plan Overview"));
    assert!(content.contains("<plan-overview>"));
    assert!(content.contains("This plan does X."));
    assert!(content.contains("</plan-overview>"));

    assert!(content.contains("## Previous Session Handoff"));
    assert!(content.contains("<handoff>"));
    assert!(content.contains("Previous session completed tasks A and B."));
    assert!(content.contains("</handoff>"));
}

#[test]
fn test_extract_plan_overview() {
    let plan_content = r#"# PLAN: Test Feature

## Overview

This is the overview section.
It has multiple lines.

## Current State

Current state description.

## Proposed Changes

Proposed changes here.

## Stages

### Stage 1: First Stage

Implementation details.

```yaml
loom:
  version: 1
```
"#;

    let overview = extract_plan_overview(plan_content).unwrap();
    assert!(overview.contains("# PLAN: Test Feature"));
    assert!(overview.contains("## Overview"));
    assert!(overview.contains("This is the overview section."));
    assert!(overview.contains("## Current State"));
    assert!(overview.contains("## Proposed Changes"));
    // Should NOT contain Stages section
    assert!(!overview.contains("### Stage 1"));
    assert!(!overview.contains("```yaml"));
}

#[test]
fn test_extract_tasks_from_description() {
    let desc1 = "- First task\n- Second task\n- Third task";
    let tasks1 = extract_tasks_from_description(desc1);
    assert_eq!(tasks1.len(), 3);
    assert_eq!(tasks1[0], "First task");

    let desc2 = "1. First task\n2. Second task\n3. Third task";
    let tasks2 = extract_tasks_from_description(desc2);
    assert_eq!(tasks2.len(), 3);
    assert_eq!(tasks2[1], "Second task");

    let desc3 = "* Task one\n* Task two";
    let tasks3 = extract_tasks_from_description(desc3);
    assert_eq!(tasks3.len(), 2);
    assert_eq!(tasks3[0], "Task one");

    let desc4 = "No tasks here";
    let tasks4 = extract_tasks_from_description(desc4);
    assert_eq!(tasks4.len(), 0);
}

#[test]
fn test_remove_signal() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(work_dir.join("signals")).unwrap();

    let signal_path = work_dir.join("signals").join("session-test-123.md");
    fs::write(&signal_path, "test content").unwrap();
    assert!(signal_path.exists());

    let result = remove_signal("session-test-123", &work_dir);
    assert!(result.is_ok());
    assert!(!signal_path.exists());

    let result2 = remove_signal("nonexistent", &work_dir);
    assert!(result2.is_ok());
}

#[test]
fn test_list_signals() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    let signals_dir = work_dir.join("signals");
    fs::create_dir_all(&signals_dir).unwrap();

    fs::write(signals_dir.join("session-1.md"), "").unwrap();
    fs::write(signals_dir.join("session-2.md"), "").unwrap();
    fs::write(signals_dir.join("session-3.md"), "").unwrap();
    fs::write(signals_dir.join("not-a-signal.txt"), "").unwrap();

    let signals = list_signals(&work_dir).unwrap();
    assert_eq!(signals.len(), 3);
    assert!(signals.contains(&"session-1".to_string()));
    assert!(signals.contains(&"session-2".to_string()));
    assert!(signals.contains(&"session-3".to_string()));
    assert!(!signals.contains(&"not-a-signal".to_string()));
}

#[test]
fn test_read_signal() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let result = read_signal("session-test-123", &work_dir);
    assert!(result.is_ok());

    let signal_content = result.unwrap();
    assert!(signal_content.is_some());

    let content = signal_content.unwrap();
    assert_eq!(content.session_id, "session-test-123");
    assert_eq!(content.stage_id, "stage-1");
    assert_eq!(content.stage_name, "Implement signals module");
    assert!(!content.acceptance_criteria.is_empty());
}

#[test]
fn test_update_signal_add_tasks() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    let updates = SignalUpdates {
        add_tasks: Some(vec!["New task 1".to_string(), "New task 2".to_string()]),
        ..Default::default()
    };

    let result = update_signal("session-test-123", updates, &work_dir);
    assert!(result.is_ok());

    let signal_path = work_dir.join("signals").join("session-test-123.md");
    let content = fs::read_to_string(signal_path).unwrap();
    assert!(content.contains("New task 1"));
    assert!(content.contains("New task 2"));
}

#[test]
fn test_generate_signal_with_git_history() {
    use crate::handoff::git_handoff::{CommitInfo, GitHistory};

    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    let git_history = GitHistory {
        branch: "loom/stage-1".to_string(),
        base_branch: "main".to_string(),
        commits: vec![CommitInfo {
            hash: "abc1234".to_string(),
            message: "Add feature".to_string(),
        }],
        uncommitted_changes: vec!["M src/test.rs".to_string()],
    };

    let result = generate_signal(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        Some(&git_history),
        &work_dir,
    );

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Git History"));
    assert!(content.contains("**Branch**: loom/stage-1 (from main)"));
    assert!(content.contains("abc1234"));
    assert!(content.contains("Add feature"));
    assert!(content.contains("M src/test.rs"));
}

#[test]
fn test_render_review_dimensions_require_all() {
    let config = CodeReviewConfig {
        dimensions: vec![
            "security".to_string(),
            "architecture".to_string(),
            "testing".to_string(),
        ],
        require_all: true,
    };

    let section = render_review_dimensions(&config).expect("non-empty dimensions should render");

    assert!(section.contains("## Review Dimensions"));
    // require_all is reflected in the framing text.
    assert!(section.contains("require_all"));
    assert!(section.contains("MUST"));
    // Each configured dimension appears as an actionable checkbox.
    assert!(section.contains("- [ ] **security**"));
    assert!(section.contains("- [ ] **architecture**"));
    assert!(section.contains("- [ ] **testing**"));
}

#[test]
fn test_render_review_dimensions_advisory() {
    let config = CodeReviewConfig {
        dimensions: vec!["security".to_string()],
        require_all: false,
    };

    let section = render_review_dimensions(&config).expect("non-empty dimensions should render");

    assert!(section.contains("## Review Dimensions"));
    // When require_all is false the framing is advisory, not mandatory.
    assert!(section.contains("where applicable"));
    assert!(!section.contains("MUST"));
    assert!(section.contains("- [ ] **security**"));
}

#[test]
fn test_render_review_dimensions_empty_is_none() {
    let config = CodeReviewConfig {
        dimensions: vec![],
        require_all: true,
    };
    assert!(render_review_dimensions(&config).is_none());
}

/// Write a plan file with an integration-verify stage carrying `code_review`
/// dimensions, plus a `config.toml` whose `source_path` points at it. Mirrors
/// the on-disk layout the daemon reads at spawn time.
fn write_plan_with_code_review(project_root: &std::path::Path, work_dir: &std::path::Path) {
    let plan_path = project_root.join("PLAN-review-dims.md");
    let plan_content = r#"# PLAN: Review Dimensions Test

---

<!-- loom METADATA - Do not edit manually -->

```yaml
loom:
  version: 1
  stages:
    - id: iv-stage
      name: "Integration Verify"
      dependencies: []
      working_dir: "."
      acceptance:
        - "true"
      code_review:
        dimensions: ["security", "architecture", "testing"]
        require_all: true
```

<!-- END loom METADATA -->
"#;
    fs::write(&plan_path, plan_content).unwrap();

    let config_content = format!("[plan]\nsource_path = \"{}\"\n", plan_path.display());
    fs::write(work_dir.join("config.toml"), config_content).unwrap();
}

#[test]
fn test_generate_signal_renders_code_review_for_integration_verify() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let work_dir = project_root.join(".work");
    fs::create_dir_all(&work_dir).unwrap();
    write_plan_with_code_review(project_root, &work_dir);

    let mut session = create_test_session();
    session.assign_to_stage("iv-stage".to_string());
    let mut stage = create_test_stage();
    stage.id = "iv-stage".to_string();
    stage.stage_type = StageType::IntegrationVerify;
    let worktree = create_test_worktree();

    let signal_path =
        generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(
        content.contains("## Review Dimensions"),
        "integration-verify signal should render configured review dimensions:\n{content}"
    );
    assert!(content.contains("- [ ] **security**"));
    assert!(content.contains("- [ ] **architecture**"));
    assert!(content.contains("- [ ] **testing**"));
    assert!(content.contains("require_all"));
}

#[test]
fn test_generate_signal_skips_code_review_for_standard_stage() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let work_dir = project_root.join(".work");
    fs::create_dir_all(&work_dir).unwrap();
    write_plan_with_code_review(project_root, &work_dir);

    // Same plan on disk, but a Standard runtime stage must NOT render the
    // review-dimensions section — the section is gated to integration-verify.
    let mut session = create_test_session();
    session.assign_to_stage("iv-stage".to_string());
    let mut stage = create_test_stage();
    stage.id = "iv-stage".to_string();
    stage.stage_type = StageType::Standard;
    let worktree = create_test_worktree();

    let signal_path =
        generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(!content.contains("## Review Dimensions"));
}
