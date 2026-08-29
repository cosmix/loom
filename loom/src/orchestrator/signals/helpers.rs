//! Shared helpers for signal generation, formatting, and parsing.
//!
//! This module consolidates duplicated patterns across the 7 signal types
//! (standard, merge, merge-conflict, knowledge, recovery, and metrics).

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// Retrieval/delivery glue (knowledge-brief lookup and its persistence record)
// lives in the `retrieval` sibling module; re-exported here so every existing
// `helpers::` call site keeps working unchanged.
pub(super) use super::retrieval::{
    ensure_trailing_newline, knowledge_tree_is_empty, persist_delivery, retrieve_stage_pack,
};

// Markdown section formatters (Target / Execution Rules / Stage Context /
// Conflicting Files / knowledge Target) live in the `section_formatters`
// sibling module - a cohesive cluster of "render one outgoing signal
// section" functions, split out to keep this grab-bag file under the line
// budget. Re-exported here so every existing `helpers::format_*_section`
// call site (merge.rs, merge_conflict.rs, knowledge.rs) keeps working
// unchanged.
pub(super) use super::section_formatters::{
    format_conflicting_files_section, format_execution_rules_section,
    format_knowledge_target_section, format_stage_context_section, format_target_section,
};

/// Handoff trigger, replacing the retired percentage-threshold line: the
/// PostToolUse hook reports the ceiling, so the agent never estimates it.
pub(super) const CONTEXT_CEILING_HANDOFF: &str = "- When the PostToolUse hook reports the context ceiling, finish the unit of work in progress and run `loom handoff --stage <id> --session <id> --trigger ceiling`, then stop.\n";

/// Write a signal file to the signals directory, creating it if needed.
///
/// Replaces the duplicated dir-create + path-build + write pattern
/// found across all signal generators.
pub(super) fn write_signal_file(
    session_id: &str,
    content: &str,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    let signal_path = signals_dir.join(format!("{session_id}.md"));

    fs::write(&signal_path, content)
        .with_context(|| format!("Failed to write signal file: {}", signal_path.display()))?;

    Ok(signal_path)
}

/// Parse markdown content into sections keyed by `## ` headers.
///
/// Returns a map from section name to the non-empty trimmed lines in that section.
/// Lines before the first `## ` header are stored under the empty string key.
///
/// Replaces the 3 near-identical section-parsing loops in merge.rs,
/// and merge_conflict.rs.
pub(super) fn parse_signal_sections(content: &str) -> HashMap<String, Vec<String>> {
    let mut sections: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_section = String::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if let Some(header) = trimmed.strip_prefix("## ") {
            current_section = header.to_string();
            sections.entry(current_section.clone()).or_default();
            continue;
        }

        if !trimmed.is_empty() {
            sections
                .entry(current_section.clone())
                .or_default()
                .push(trimmed.to_string());
        }
    }

    sections
}

/// Extract a markdown bold field value from a list of lines.
///
/// Looks for lines matching `- **{field}**: value` and returns the value.
/// Useful in combination with `parse_signal_sections` for extracting
/// specific fields from a section.
pub(super) fn extract_field_from_lines<'a>(lines: &'a [String], field: &str) -> Option<&'a str> {
    let prefix = format!("- **{field}**: ");
    for line in lines {
        if let Some(value) = line.strip_prefix(&prefix) {
            return Some(value);
        }
    }
    None
}

/// Extract backtick-wrapped items from a list of markdown bullet lines.
///
/// Parses lines like `- \`path/to/file\`` and returns the unwrapped values.
/// Useful for extracting file lists from "Conflicting Files" or "Source Branches" sections.
pub(super) fn extract_backtick_items(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| {
            line.strip_prefix("- `")
                .and_then(|rest| rest.strip_suffix('`'))
                .map(|s| s.to_string())
        })
        .collect()
}

/// Append the commit-timing doctrine: commits are the ORCHESTRATOR's, made ONLY
/// as the final step of the stage, after every subagent has returned and all
/// verification is green — never mid-stage.
///
/// `gate` names the verification gate for the stage type (code stages: build,
/// tests, lint, format, acceptance; documentation stages: acceptance only) and
/// `review` states the review condition (code stages: the mini adversarial
/// review returned and its findings fixed; documentation stages: the knowledge
/// files re-read). Both are interpolated into otherwise identical text so the
/// doctrine lives in exactly one place; `tests_commit_timing.rs` pins the
/// sentinel phrases across every stable prefix and `CLAUDE.md.template`.
pub(super) fn append_commit_timing_rules(content: &mut String, gate: &str, review: &str) {
    content.push_str(
        "**When to Commit (ORCHESTRATOR ONLY — AT THE END — AFTER ALL VERIFICATION):**\n\n",
    );
    content.push_str("Commits are yours alone and ONLY as the final step of the stage — never mid-stage, never \"what is done so far\". Legitimate only once ALL of these hold:\n\n");
    content.push_str("1. Every subagent, coordinator, team, and Workflow you spawned has RETURNED and its result is absorbed.\n");
    content.push_str(&format!(
        "2. The full verification gate is GREEN on the complete tree: {gate}.\n"
    ));
    content.push_str(&format!("3. {review}\n\n"));
    content.push_str("Then stage your files, commit (one logical commit per concern — module, tests, wiring, docs), and run `loom stage complete <stage-id>`.\n\n");
}

/// Append the shared "settled stage" completion doctrine: `loom stage
/// complete` is the session's LAST act, run only once the stage is SETTLED,
/// plus the "verify acceptance criteria" line shared by both callers.
///
/// Duplicated verbatim between `append_completion_rules` (standard and
/// integration-verify prefixes) and `generate_knowledge_stable_prefix`
/// (`cache.rs`) - extracted here so the doctrine text lives in exactly one
/// place.
pub(super) fn append_settled_completion_rules(content: &mut String) {
    content.push_str(
        "- **`loom stage complete` is the LAST act of your session.** Run it ONLY when the stage is SETTLED: every subagent returned and absorbed, every defect found by review or verification fixed and re-verified, all work committed (`git status` clean).\n",
    );
    content.push_str(
        "- **After it succeeds: STOP and end the session** — post-completion work is LOST WORK (the merge starts from the completed commit).\n",
    );
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
}

/// Append completion rules shared between standard and integration-verify prefixes
pub(super) fn append_completion_rules(content: &mut String) {
    append_settled_completion_rules(content);
    content.push_str(CONTEXT_CEILING_HANDOFF);
    content.push_str("- Run `loom stage complete <stage-id>` from the worktree ROOT directory; if acceptance criteria fail, fix and run it again\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_signal_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let work_dir = tmp.path();

        let path = write_signal_file("session-123", "test content", work_dir).unwrap();
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "test content");
        assert_eq!(path, work_dir.join("signals").join("session-123.md"));
    }

    #[test]
    fn test_write_signal_file_creates_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let work_dir = tmp.path();
        let signals_dir = work_dir.join("signals");
        assert!(!signals_dir.exists());

        write_signal_file("session-456", "content", work_dir).unwrap();
        assert!(signals_dir.exists());
    }

    #[test]
    fn test_parse_signal_sections() {
        let content = r#"# Header

Some preamble

## Target

- **Session**: session-123
- **Stage**: my-stage

## Conflicting Files

- `src/main.rs`
- `src/lib.rs`

## Empty Section
"#;
        let sections = parse_signal_sections(content);

        let target = sections.get("Target").unwrap();
        assert_eq!(target.len(), 2);
        assert!(target[0].contains("Session"));
        assert!(target[1].contains("Stage"));

        let files = sections.get("Conflicting Files").unwrap();
        assert_eq!(files.len(), 2);

        let empty = sections.get("Empty Section").unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_extract_field_from_lines() {
        let lines = vec![
            "- **Session**: session-123".to_string(),
            "- **Stage**: my-stage".to_string(),
            "- **Target Branch**: main".to_string(),
        ];

        assert_eq!(extract_field_from_lines(&lines, "Stage"), Some("my-stage"));
        assert_eq!(
            extract_field_from_lines(&lines, "Target Branch"),
            Some("main")
        );
        assert_eq!(extract_field_from_lines(&lines, "Missing"), None);
    }

    #[test]
    fn test_extract_backtick_items() {
        let lines = vec![
            "- `src/main.rs`".to_string(),
            "- `src/lib.rs`".to_string(),
            "Some other line".to_string(),
        ];

        let items = extract_backtick_items(&lines);
        assert_eq!(items, vec!["src/main.rs", "src/lib.rs"]);
    }
}
