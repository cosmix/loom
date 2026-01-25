//! Signal compression and caching tests

use std::fs;
use tempfile::TempDir;

use super::super::cache::{compute_hash, generate_stable_prefix, SignalMetrics};
use super::super::format::{format_signal_with_metrics, format_signal_content};
use super::super::generate::generate_signal_with_metrics;
use super::{create_test_session, create_test_stage, create_test_worktree};
use super::super::types::EmbeddedContext;

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
