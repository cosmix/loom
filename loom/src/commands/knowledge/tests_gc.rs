//! Tests for `commands/knowledge/gc.rs`.

use super::*;
use crate::commands::knowledge::tests::setup_test_env;
use crate::fs::knowledge::{KnowledgeFile, Tier1Metrics};
use serial_test::serial;

fn fake_metrics_recommended() -> GcMetrics {
    GcMetrics {
        layout: KnowledgeLayout::Hierarchical,
        total_lines: 1000,
        tier1: vec![Tier1Metrics {
            file_type: KnowledgeFile::Architecture,
            line_count: 500,
            duplicate_headers: vec!["## Overview".to_string()],
            promoted_block_count: 5,
            oversized_sections: vec![("Merge Flow".to_string(), 80)],
            broken_links: Vec::new(),
            has_issues: true,
        }],
        topics: Vec::new(),
        index_stale: false,
        gc_recommended: true,
        reasons: vec![
            "architecture.md has an oversized section 'Merge Flow' (80 lines)".to_string(),
        ],
    }
}

#[test]
fn test_gc_system_prompt_dry_run_includes_dry_run_clause() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("sonnet", true, false, &metrics);
    assert!(prompt.contains("DRY-RUN"));
    assert!(prompt.contains("MUST NOT write"));
    assert!(!prompt.contains("Mode: RESTRUCTURE"));
}

#[test]
fn test_gc_system_prompt_restructure_mode() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("sonnet", false, false, &metrics);
    assert!(prompt.contains("Mode: RESTRUCTURE"));
    assert!(prompt.contains("Edit knowledge files directly"));
    assert!(!prompt.contains("DRY-RUN"));
}

#[test]
fn test_gc_system_prompt_includes_targets() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("sonnet", false, false, &metrics);
    assert!(prompt.contains("architecture.md"));
    assert!(prompt.contains("500 lines"));
}

#[test]
fn test_gc_system_prompt_recursion_warning() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("sonnet", false, false, &metrics);
    assert!(prompt.contains("do NOT run `loom knowledge gc`"));
}

#[test]
fn test_gc_system_prompt_does_not_embed_file_contents() {
    // Regression: the gc system prompt must NOT embed knowledge file
    // contents — that overflows Linux's per-argv-entry limit.
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("sonnet", false, false, &metrics);
    assert!(!prompt.contains("Existing Knowledge"));
    assert!(prompt.contains("Read them directly"));
}

#[test]
fn test_gc_initial_prompt_embeds_model() {
    let prompt = build_gc_initial_prompt("opus", false, false);
    assert!(prompt.contains("model \"opus\""));
    assert!(prompt.contains("Restructure the files via Edit/Write"));
}

#[test]
fn test_gc_initial_prompt_dry_run() {
    let prompt = build_gc_initial_prompt("sonnet", true, false);
    assert!(prompt.contains("Do NOT write"));
}

#[test]
fn test_gc_initial_prompt_uses_agent_team() {
    let prompt = build_gc_initial_prompt("opus", false, false);
    assert!(prompt.contains("agent team"));
}

#[test]
fn test_gc_system_prompt_uses_agent_team() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("opus", false, false, &metrics);
    assert!(prompt.contains("agent team"));
    assert!(prompt.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"));
}

#[test]
fn test_gc_system_prompt_protects_self_improvement_content() {
    // Recorded mistakes / gotchas / prevention rules are the highest-value
    // content — GC must condense but never drop them.
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("opus", false, false, &metrics);
    assert!(prompt.contains("prevention rules"));
}

#[test]
fn test_gc_system_prompt_never_delete_to_hit_line_count() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("opus", false, false, &metrics);
    assert!(prompt.contains("NEVER delete a lesson to hit a line count"));
    assert!(prompt.contains("EXTRACT"));
}

#[test]
fn test_gc_system_prompt_states_no_total_lines_budget() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("opus", false, false, &metrics);
    assert!(prompt.contains("no total-lines budget"));
}

#[test]
fn test_gc_system_prompt_legacy_migration_sentence() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("opus", false, true, &metrics);
    assert!(prompt.contains("Legacy Migration"));
    assert!(prompt.contains("migration"));
    assert!(prompt.to_lowercase().contains("legacy"));
}

#[test]
fn test_gc_system_prompt_no_legacy_clause_when_hierarchical() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("opus", false, false, &metrics);
    assert!(!prompt.contains("Legacy Migration"));
}

#[test]
fn test_gc_system_prompt_mandatory_index_step() {
    let metrics = fake_metrics_recommended();
    let prompt = build_gc_system_prompt("opus", false, false, &metrics);
    assert!(prompt.contains("loom knowledge index"));
}

#[test]
fn test_gc_initial_prompt_legacy_migration_note() {
    let prompt = build_gc_initial_prompt("opus", false, true);
    assert!(prompt.contains("migrates it into the tiered"));
}

#[test]
#[serial]
fn test_gc_bails_when_clean() {
    // When knowledge is clean (no GC recommended), gc() must return Ok
    // without attempting to spawn Claude. We can't easily intercept the
    // spawn, so we just ensure the early-return path executes without error
    // on an initialized-but-empty knowledge dir.
    let (_temp_dir, test_dir) = setup_test_env();
    let original_dir = std::env::current_dir().expect("Failed to get current dir");
    std::env::set_current_dir(&test_dir).expect("Failed to change dir");

    crate::commands::knowledge::init().expect("Failed to init knowledge");
    let result = gc(None, true, true);
    assert!(result.is_ok());

    std::env::set_current_dir(original_dir).expect("Failed to restore dir");
}
