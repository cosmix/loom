//! Shared helpers for signal generation, formatting, and parsing.
//!
//! This module consolidates duplicated patterns across the 7 signal types
//! (standard, merge, base_conflict, merge_conflict, knowledge, recovery, metrics).

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::stage::Stage;

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

/// Format the "## Target" markdown section for conflict-type signals.
///
/// Shared across merge, base_conflict, and merge_conflict signal generators.
/// Standard stage signals have a more complex target section (with working_dir,
/// execution path, etc.) and use their own formatter in `format/sections.rs`.
pub(super) fn format_target_section(
    session_id: &str,
    stage_id: &str,
    source_branch: Option<&str>,
    target_branch: &str,
) -> String {
    let mut content = String::new();

    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {session_id}\n"));
    content.push_str(&format!("- **Stage**: {stage_id}\n"));
    if let Some(branch) = source_branch {
        content.push_str(&format!("- **Source Branch**: {branch}\n"));
    }
    content.push_str(&format!("- **Target Branch**: {target_branch}\n"));
    content.push('\n');

    content
}

/// Format the "## Execution Rules" section for conflict resolution signals.
///
/// The `preserve_intent` parameter controls the wording:
/// - `"BOTH branches"` for merge and merge_conflict signals
/// - `"ALL branches"` for base_conflict signals (multiple dependency branches)
pub(super) fn format_execution_rules_section(preserve_intent: &str) -> String {
    let mut content = String::new();

    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` rules. Key reminders:\n");
    content.push_str("- **Do NOT modify code** beyond what's needed for conflict resolution\n");
    content.push_str(&format!(
        "- **Preserve intent from {preserve_intent}** where possible\n"
    ));
    content.push_str("- **Ask the user** if unclear how to resolve a conflict\n");
    content.push_str("- **Use TodoWrite** to track resolution progress\n\n");

    content
}

/// Format the "## Stage Context" section showing stage name and description.
///
/// Returns an empty string if the stage has no description.
/// Shared across merge and base_conflict signal generators.
pub(super) fn format_stage_context_section(stage: &Stage) -> String {
    if let Some(desc) = &stage.description {
        format!("## Stage Context\n\n**{}**: {}\n\n", stage.name, desc)
    } else {
        String::new()
    }
}

/// Format the "## Conflicting Files" section as a bullet list of backtick-wrapped paths.
///
/// Shows a fallback message when no files are listed.
/// Shared across merge, base_conflict, and merge_conflict signal generators.
pub(super) fn format_conflicting_files_section(files: &[String]) -> String {
    let mut content = String::new();

    content.push_str("## Conflicting Files\n\n");
    if files.is_empty() {
        content
            .push_str("_No specific files listed - run `git status` to see current conflicts_\n");
    } else {
        for file in files {
            content.push_str(&format!("- `{file}`\n"));
        }
    }
    content.push('\n');

    content
}

/// Format the "## Target" markdown section for knowledge stage signals.
///
/// Knowledge stages run in the main repo (no worktree / no source branch), so
/// their Target section uses Type and Directory fields instead of branches.
pub(super) fn format_knowledge_target_section(
    session_id: &str,
    stage_id: &str,
    plan_id: Option<&str>,
    repo_root: &str,
) -> String {
    let mut content = String::new();

    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {session_id}\n"));
    content.push_str(&format!("- **Stage**: {stage_id}\n"));
    content.push_str("- **Type**: Knowledge (no worktree)\n");
    if let Some(plan) = plan_id {
        content.push_str(&format!("- **Plan**: {plan}\n"));
    }
    content.push_str(&format!("- **Directory**: {repo_root}\n"));
    content.push('\n');

    content
}

/// Parse markdown content into sections keyed by `## ` headers.
///
/// Returns a map from section name to the non-empty trimmed lines in that section.
/// Lines before the first `## ` header are stored under the empty string key.
///
/// Replaces the 3 near-identical section-parsing loops in merge.rs,
/// base_conflict.rs, and merge_conflict.rs.
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
    fn test_format_target_section_with_source() {
        let section = format_target_section("session-1", "stage-1", Some("loom/stage-1"), "main");
        assert!(section.contains("## Target"));
        assert!(section.contains("- **Session**: session-1"));
        assert!(section.contains("- **Stage**: stage-1"));
        assert!(section.contains("- **Source Branch**: loom/stage-1"));
        assert!(section.contains("- **Target Branch**: main"));
    }

    #[test]
    fn test_format_target_section_without_source() {
        let section = format_target_section("session-1", "stage-1", None, "loom/_base/stage-1");
        assert!(!section.contains("Source Branch"));
        assert!(section.contains("- **Target Branch**: loom/_base/stage-1"));
    }

    #[test]
    fn test_format_execution_rules_both() {
        let rules = format_execution_rules_section("BOTH branches");
        assert!(rules.contains("Preserve intent from BOTH branches"));
        assert!(rules.contains("Do NOT modify code"));
    }

    #[test]
    fn test_format_execution_rules_all() {
        let rules = format_execution_rules_section("ALL branches");
        assert!(rules.contains("Preserve intent from ALL branches"));
    }

    #[test]
    fn test_format_stage_context_with_description() {
        let mut stage = Stage::new("My Stage".to_string(), Some("Description here".to_string()));
        stage.id = "my-stage".to_string();
        let section = format_stage_context_section(&stage);
        assert!(section.contains("## Stage Context"));
        assert!(section.contains("**My Stage**: Description here"));
    }

    #[test]
    fn test_format_stage_context_no_description() {
        let mut stage = Stage::new("My Stage".to_string(), None);
        stage.id = "my-stage".to_string();
        let section = format_stage_context_section(&stage);
        assert!(section.is_empty());
    }

    #[test]
    fn test_format_conflicting_files() {
        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let section = format_conflicting_files_section(&files);
        assert!(section.contains("## Conflicting Files"));
        assert!(section.contains("- `src/main.rs`"));
        assert!(section.contains("- `src/lib.rs`"));
    }

    #[test]
    fn test_format_conflicting_files_empty() {
        let section = format_conflicting_files_section(&[]);
        assert!(section.contains("_No specific files listed"));
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
