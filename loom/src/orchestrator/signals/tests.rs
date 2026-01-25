use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::models::worktree::Worktree;

use super::crud::{list_signals, read_signal, remove_signal, update_signal};
use super::format::{extract_tasks_from_description, format_signal_content};
use super::generate::{extract_plan_overview, generate_signal};
use super::merge::{
    format_merge_signal_content, generate_merge_signal, parse_merge_signal_content,
    read_merge_signal,
};
use super::types::{DependencyStatus, EmbeddedContext, SignalUpdates};

fn create_test_session() -> Session {
    let mut session = Session::new();
    session.id = "session-test-123".to_string();
    session.assign_to_stage("stage-1".to_string());
    session
}

fn create_test_stage() -> Stage {
    let mut stage = Stage::new(
        "Implement signals module".to_string(),
        Some("Create signal file generation logic".to_string()),
    );
    stage.id = "stage-1".to_string();
    stage.status = StageStatus::Executing;
    stage.add_acceptance_criterion("Signal files are generated correctly".to_string());
    stage.add_acceptance_criterion("All tests pass".to_string());
    stage.add_file_pattern("src/orchestrator/signals.rs".to_string());
    stage
}

fn create_test_worktree() -> Worktree {
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
        knowledge_summary: None,
        knowledge_exists: false,
        knowledge_is_empty: true,
        task_state: None,
        memory_content: None,
        skill_recommendations: Vec::new(),
        context_budget: None,
        context_usage: None,
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
fn test_generate_merge_signal_basic() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let conflicting_files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];

    let result = generate_merge_signal(
        &session,
        &stage,
        "loom/stage-1",
        "main",
        &conflicting_files,
        &work_dir,
    );

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    assert!(signal_path.exists());

    let content = fs::read_to_string(&signal_path).unwrap();
    assert!(content.contains("# Merge Signal: session-test-123"));
    assert!(content.contains("- **Session**: session-test-123"));
    assert!(content.contains("- **Stage**: stage-1"));
    assert!(content.contains("- **Source Branch**: loom/stage-1"));
    assert!(content.contains("- **Target Branch**: main"));
    assert!(content.contains("## Conflicting Files"));
    assert!(content.contains("- `src/main.rs`"));
    assert!(content.contains("- `src/lib.rs`"));
}

#[test]
fn test_generate_merge_signal_empty_conflicts() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();

    let result = generate_merge_signal(&session, &stage, "loom/stage-1", "main", &[], &work_dir);

    assert!(result.is_ok());
    let signal_path = result.unwrap();
    let content = fs::read_to_string(&signal_path).unwrap();

    assert!(content.contains("## Conflicting Files"));
    assert!(content.contains("_No specific files listed"));
}

#[test]
fn test_format_merge_signal_content_sections() {
    let session = create_test_session();
    let stage = create_test_stage();
    let conflicting_files = vec!["src/test.rs".to_string()];

    let content =
        format_merge_signal_content(&session, &stage, "loom/stage-1", "main", &conflicting_files);

    // Check all required sections are present
    assert!(content.contains("# Merge Signal:"));
    assert!(content.contains("## Merge Context"));
    assert!(content.contains("## Execution Rules"));
    assert!(content.contains("## Target"));
    assert!(content.contains("## Stage Context"));
    assert!(content.contains("## Conflicting Files"));
    assert!(content.contains("## Your Task"));
    assert!(content.contains("## Important"));

    // Check key instructions
    assert!(content.contains("git merge loom/stage-1"));
    assert!(content.contains("Resolve conflicts"));
    assert!(content.contains("git add"));
    assert!(content.contains("git commit"));
    // Should use worktree remove for cleanup, not loom merge
    assert!(content.contains("loom worktree remove stage-1"));
}

#[test]
fn test_read_merge_signal() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let conflicting_files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];

    generate_merge_signal(
        &session,
        &stage,
        "loom/stage-1",
        "main",
        &conflicting_files,
        &work_dir,
    )
    .unwrap();

    let result = read_merge_signal("session-test-123", &work_dir);
    assert!(result.is_ok());

    let signal_content = result.unwrap();
    assert!(signal_content.is_some());

    let content = signal_content.unwrap();
    assert_eq!(content.session_id, "session-test-123");
    assert_eq!(content.stage_id, "stage-1");
    assert_eq!(content.source_branch, "loom/stage-1");
    assert_eq!(content.target_branch, "main");
    assert_eq!(content.conflicting_files.len(), 2);
    assert!(content
        .conflicting_files
        .contains(&"src/main.rs".to_string()));
    assert!(content
        .conflicting_files
        .contains(&"src/lib.rs".to_string()));
}

#[test]
fn test_read_merge_signal_returns_none_for_regular_signal() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    // Generate a regular signal (not a merge signal)
    generate_signal(&session, &stage, &worktree, &[], None, None, &work_dir).unwrap();

    // read_merge_signal should return None for regular signals
    let result = read_merge_signal("session-test-123", &work_dir);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_read_merge_signal_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let result = read_merge_signal("nonexistent-session", &work_dir);
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[test]
fn test_parse_merge_signal_content() {
    let content = r#"# Merge Signal: session-merge-123

## Merge Context

You are resolving a **merge conflict** in the main repository.

## Target

- **Session**: session-merge-123
- **Stage**: feature-stage
- **Source Branch**: loom/feature-stage
- **Target Branch**: develop

## Conflicting Files

- `src/app.rs`
- `src/config.rs`

## Your Task

1. Run: `git merge loom/feature-stage`
"#;

    let result = parse_merge_signal_content("session-merge-123", content);
    assert!(result.is_ok());

    let parsed = result.unwrap();
    assert_eq!(parsed.session_id, "session-merge-123");
    assert_eq!(parsed.stage_id, "feature-stage");
    assert_eq!(parsed.source_branch, "loom/feature-stage");
    assert_eq!(parsed.target_branch, "develop");
    assert_eq!(parsed.conflicting_files.len(), 2);
    assert_eq!(parsed.conflicting_files[0], "src/app.rs");
    assert_eq!(parsed.conflicting_files[1], "src/config.rs");
}

// ============================================================================
// Signal Compression & Caching Tests
// ============================================================================

use super::cache::{compute_hash, generate_stable_prefix, SignalMetrics};
use super::format::format_signal_with_metrics;
use super::generate::generate_signal_with_metrics;

#[test]
fn test_compute_hash_is_deterministic() {
    let content = "test content for hashing";
    let hash1 = compute_hash(content);
    let hash2 = compute_hash(content);
    assert_eq!(hash1, hash2);
    assert_eq!(hash1.len(), 16); // 8 bytes as hex = 16 chars
}

#[test]
fn test_compute_hash_different_for_different_content() {
    let hash1 = compute_hash("content A");
    let hash2 = compute_hash("content B");
    assert_ne!(hash1, hash2);
}

#[test]
fn test_stable_prefix_is_constant() {
    let prefix1 = generate_stable_prefix();
    let prefix2 = generate_stable_prefix();
    assert_eq!(
        prefix1, prefix2,
        "Stable prefix should be identical across calls"
    );
}

#[test]
fn test_stable_prefix_contains_required_content() {
    let prefix = generate_stable_prefix();

    // Must contain isolation rules
    assert!(prefix.contains("Worktree Context"));
    assert!(prefix.contains("Isolation Boundaries"));
    assert!(prefix.contains("CONFINED"));
    assert!(prefix.contains("FORBIDDEN"));

    // Must contain execution rules
    assert!(prefix.contains("Execution Rules"));
    assert!(prefix.contains("STAY IN THIS WORKTREE"));
    assert!(prefix.contains("git add <specific-files>"));
}

#[test]
fn test_signal_metrics_calculation() {
    let stable = "stable content";
    let semi_stable = "semi-stable";
    let dynamic = "dynamic";
    let recitation = "recite";

    let metrics = SignalMetrics::from_sections(stable, semi_stable, dynamic, recitation);

    assert_eq!(metrics.stable_prefix_bytes, stable.len());
    assert_eq!(metrics.semi_stable_bytes, semi_stable.len());
    assert_eq!(metrics.dynamic_bytes, dynamic.len());
    assert_eq!(metrics.recitation_bytes, recitation.len());

    let total = stable.len() + semi_stable.len() + dynamic.len() + recitation.len();
    assert_eq!(metrics.signal_size_bytes, total);
    assert_eq!(metrics.estimated_tokens, total / 4);
}

#[test]
fn test_format_signal_with_metrics() {
    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let formatted = format_signal_with_metrics(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Verify content is generated
    assert!(formatted.content.contains("# Signal: session-test-123"));
    assert!(formatted.content.contains("## Worktree Context"));
    assert!(formatted.content.contains("## Immediate Tasks"));

    // Verify metrics are populated
    assert!(formatted.metrics.signal_size_bytes > 0);
    assert!(formatted.metrics.stable_prefix_bytes > 0);
    assert!(!formatted.metrics.stable_prefix_hash.is_empty());
}

#[test]
fn test_generate_signal_with_metrics() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path().join(".work");
    fs::create_dir_all(&work_dir).unwrap();

    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();

    let result =
        generate_signal_with_metrics(&session, &stage, &worktree, &[], None, None, &work_dir);

    assert!(result.is_ok());
    let (signal_path, metrics) = result.unwrap();

    // Verify file was created
    assert!(signal_path.exists());

    // Verify metrics
    assert!(metrics.signal_size_bytes > 0);
    assert!(metrics.stable_prefix_bytes > 0);
    assert!(metrics.estimated_tokens > 0);
    assert!(!metrics.stable_prefix_hash.is_empty());

    // Content should match metrics size
    let content = fs::read_to_string(&signal_path).unwrap();
    assert_eq!(content.len(), metrics.signal_size_bytes);
}

#[test]
fn test_signal_sections_ordering() {
    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext {
        memory_content: Some("Test memory content".to_string()),
        context_budget: None,
        context_usage: None,
        ..Default::default()
    };

    let formatted = format_signal_with_metrics(
        &session,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    let content = &formatted.content;

    // Verify section ordering (Manus pattern):
    // 1. STABLE: Worktree Context, Execution Rules
    // 2. SEMI-STABLE: Knowledge, Facts
    // 3. DYNAMIC: Target, Assignment, Acceptance
    // 4. RECITATION: Immediate Tasks, Session Memory (at END)

    let worktree_pos = content.find("## Worktree Context").unwrap();
    let execution_pos = content.find("## Execution Rules").unwrap();
    let knowledge_pos = content.find("## Knowledge Management").unwrap();
    let target_pos = content.find("## Target").unwrap();
    let tasks_pos = content.find("## Immediate Tasks").unwrap();
    let memory_pos = content.find("## Session Memory").unwrap();

    // Stable before semi-stable
    assert!(worktree_pos < knowledge_pos);
    assert!(execution_pos < knowledge_pos);

    // Semi-stable before dynamic
    assert!(knowledge_pos < target_pos);

    // Recitation at end (tasks and memory are last)
    assert!(target_pos < tasks_pos);
    assert!(tasks_pos < memory_pos);
}

#[test]
fn test_signal_contains_knowledge_management_section_empty() {
    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();
    // Default context has no knowledge (knowledge_exists: false, knowledge_is_empty: true)
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

    // Knowledge Management section should always be present
    assert!(content.contains("## Knowledge Management"));
    // For empty knowledge, should show CRITICAL warning box
    assert!(content.contains("CRITICAL: KNOWLEDGE BASE IS EMPTY"));
    assert!(content.contains("Before implementing ANYTHING"));
    // Should show exploration order
    assert!(content.contains("Exploration Order"));
    assert!(content.contains("Entry Points First"));
    assert!(content.contains("Core Modules"));
    // Commands should always be present
    assert!(content.contains("loom knowledge update entry-points"));
    assert!(content.contains("loom knowledge update patterns"));
    assert!(content.contains("loom knowledge update conventions"));
}

#[test]
fn test_signal_contains_knowledge_management_section_populated() {
    let session = create_test_session();
    let stage = create_test_stage();
    let worktree = create_test_worktree();
    // Context with populated knowledge
    let embedded_context = EmbeddedContext {
        knowledge_exists: true,
        knowledge_is_empty: false,
        knowledge_summary: Some("## Entry Points\n\n- src/main.rs".to_string()),
        context_budget: None,
        context_usage: None,
        ..Default::default()
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

    // Knowledge Management section should always be present
    assert!(content.contains("## Knowledge Management"));
    // For populated knowledge, should NOT show CRITICAL warning
    assert!(!content.contains("CRITICAL: KNOWLEDGE BASE IS EMPTY"));
    // Should show standard guidance for established codebases
    assert!(content.contains("Extend the knowledge base"));
    assert!(content.contains("undocumented modules"));
    assert!(content.contains("new insights"));
    // Commands should always be present
    assert!(content.contains("loom knowledge update entry-points"));
    assert!(content.contains("loom knowledge update patterns"));
    assert!(content.contains("loom knowledge update conventions"));
}

#[test]
fn test_stable_prefix_hash_changes_with_session() {
    // The stable prefix includes the session header, so different sessions
    // will have different hashes (but the execution rules portion is stable)
    let session1 = create_test_session();
    let mut session2 = create_test_session();
    session2.id = "session-different".to_string();

    let stage = create_test_stage();
    let worktree = create_test_worktree();
    let embedded_context = EmbeddedContext::default();

    let formatted1 = format_signal_with_metrics(
        &session1,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    let formatted2 = format_signal_with_metrics(
        &session2,
        &stage,
        &worktree,
        &[],
        None,
        None,
        &embedded_context,
    );

    // Different sessions should have different hashes (header includes session ID)
    assert_ne!(
        formatted1.metrics.stable_prefix_hash,
        formatted2.metrics.stable_prefix_hash
    );

    // But the stable portion size should be similar (only header differs)
    let size_diff = (formatted1.metrics.stable_prefix_bytes as i64
        - formatted2.metrics.stable_prefix_bytes as i64)
        .abs();
    assert!(size_diff < 100, "Stable prefix size should be similar");
}

// ============================================================================
// working_dir and Execution Path Tests
// ============================================================================

#[test]
fn test_signal_contains_working_dir() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
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

    // Check working_dir is displayed in Target section
    assert!(content.contains("working_dir"));
    assert!(content.contains("`loom`"));
}

#[test]
fn test_signal_contains_execution_path() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
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

    // Check Execution Path is displayed
    assert!(content.contains("Execution Path"));
    // Should contain the computed path: worktree.path + working_dir
    assert!(content.contains("/repo/.worktrees/stage-1/loom"));
}

#[test]
fn test_signal_execution_path_default_working_dir() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = None; // Default to "."
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

    // Check working_dir defaults to "."
    assert!(content.contains("working_dir"));
    assert!(content.contains("`.`"));
    // Execution path should just be worktree path
    assert!(content.contains("/repo/.worktrees/stage-1"));
}

#[test]
fn test_signal_acceptance_criteria_working_dir_note() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
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

    // Check acceptance criteria section contains working_dir note
    assert!(content.contains("## Acceptance Criteria"));
    assert!(content.contains("These commands will run from working_dir"));
    assert!(content.contains("`loom`"));
}

#[test]
fn test_signal_contains_where_commands_execute_box() {
    let session = create_test_session();
    let mut stage = create_test_stage();
    stage.working_dir = Some("loom".to_string());
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

    // Check the reminder box is present
    assert!(content.contains("WHERE COMMANDS EXECUTE"));
    assert!(content.contains("Acceptance criteria run from"));
    assert!(content.contains("WORKTREE + working_dir"));
}

#[test]
fn test_stable_prefix_contains_working_dir_reminder() {
    let prefix = generate_stable_prefix();

    // Check working_dir reminder is in Path Boundaries section
    assert!(prefix.contains("working_dir Reminder"));
    assert!(prefix.contains("WORKTREE + working_dir"));
    assert!(prefix.contains("execution path"));
}
