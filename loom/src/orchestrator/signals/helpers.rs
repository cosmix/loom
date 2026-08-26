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

/// Render `body` as one complete markdown list item opened by `marker`.
///
/// Every line after the first is indented to the list's continuation column
/// (the width of `marker`), so the item's later paragraphs stay INSIDE the
/// item. Left at column 0 they close the list, and a following `2. ` then reads
/// as lazy continuation text of that paragraph rather than as the second item —
/// the ladder silently loses the numbering an agent is told to follow in order.
///
/// The item is blank-line terminated, so whatever the caller appends next
/// starts a new item rather than continuing this one. Blank lines are emitted
/// empty rather than as runs of trailing spaces.
pub(super) fn as_list_item(marker: &str, body: &str) -> String {
    let indent = " ".repeat(marker.chars().count());
    let mut out = String::new();
    for (index, line) in body.lines().enumerate() {
        if index == 0 {
            out.push_str(marker);
        } else if !line.is_empty() {
            out.push_str(&indent);
        }
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    out
}

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
    content.push_str("Commits are made by YOU, the main agent, and ONLY as the final step of the stage — never mid-stage, never \"what is done so far\". A commit is legitimate only once ALL of these hold:\n\n");
    content.push_str("1. Every subagent, coordinator, team, and Workflow you spawned has RETURNED and its result is absorbed — nothing is still running or still expected to report.\n");
    content.push_str(&format!(
        "2. The full verification gate is GREEN on the complete tree: {gate}.\n"
    ));
    content.push_str(&format!("3. {review}\n\n"));
    content.push_str("Only then: stage your files, commit (one logical commit per concern — module, tests, wiring, docs), and run `loom stage complete <stage-id>`.\n\n");
    content.push_str("Committing while a subagent is still out, before the reviewer has reported, or before the gate is green is PREMATURE: it snapshots unverified work and tempts you to complete on top of it. \"I ran the tests before committing\" is no defense while any condition above is still open. A context handoff is not a reason to commit unverified work either — record the uncommitted files in the handoff; the next session resumes from the worktree.\n\n");
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
        "- **`loom stage complete` is the LAST act of your session.** Run it ONLY when the stage is SETTLED:\n",
    );
    content.push_str(
        "  - every subagent/team/Workflow you spawned has returned and its result is absorbed - nothing is still running or expected to report\n",
    );
    content.push_str(
        "  - every defect found by review or verification is fixed and re-verified - nothing is left open or merely \"reported\"\n",
    );
    content.push_str("  - all work is committed (`git status` clean)\n");
    content.push_str(
        "- **After `loom stage complete` succeeds: STOP and end the session.** No further edits, spawns, or checks - post-completion work is LOST WORK (the merge starts from the completed commit)\n",
    );
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
}

/// Append completion rules shared between standard and integration-verify prefixes
pub(super) fn append_completion_rules(content: &mut String) {
    append_settled_completion_rules(content);
    content.push_str("- **Create handoff** if context exceeds 75%\n");
    content.push_str("- **IMPORTANT: Before running `loom stage complete`, ensure you are at the worktree root directory**\n");
    content.push_str("- **If acceptance criteria fail**: Fix the issues and run `loom stage complete <stage-id>` again\n");
    content.push_str("- **NEVER use `loom stage retry` from an active session** — it creates a parallel session\n\n");
}

/// Append git staging rules with danger box (standard prefix only)
pub(super) fn append_git_staging_full(content: &mut String) {
    content.push_str("**Git Staging (CRITICAL - READ CAREFULLY):**\n\n");
    content.push_str("```text\n");
    content.push_str("  ⛔ DANGER: .work is a SYMLINK to shared state in worktrees\n");
    content.push_str("     Committing it CORRUPTS the main repository!\n");
    content.push_str("```\n\n");
    append_git_staging_rules(content);
    content.push_str("**Example:**\n");
    content.push_str("```bash\n");
    content.push_str("# CORRECT:\n");
    content.push_str("git add src/main.rs src/lib.rs tests/\n\n");
    content.push_str("# WRONG (will stage .work):\n");
    content.push_str("git add -A  # DON'T DO THIS\n");
    content.push_str("git add .   # DON'T DO THIS\n");
    content.push_str("```\n\n");
}

/// Append the 3 core git staging rules (shared by standard and integration-verify)
pub(super) fn append_git_staging_rules(content: &mut String) {
    content
        .push_str("- **ALWAYS** use `git add <specific-files>` - stage only files you modified\n");
    content.push_str("- **NEVER** use `git add -A`, `git add --all`, or `git add .`\n");
    content
        .push_str("- **NEVER** stage `.work` - it is orchestration state shared across stages\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_list_item_indents_continuation_paragraphs_to_the_marker_width() {
        let body = "First line.\n\nSecond paragraph.\n\n    indented code\n";
        let rendered = as_list_item("1. ", body);

        assert_eq!(
            rendered,
            "1. First line.\n\n   Second paragraph.\n\n       indented code\n\n"
        );
        // A following `2. ` must be the only line at column 0, or the list ends.
        let stray = rendered
            .lines()
            .skip(1)
            .find(|line| !line.is_empty() && !line.starts_with("   "));
        assert!(stray.is_none(), "unindented continuation line: {stray:?}");
    }

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
