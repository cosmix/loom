//! Merge conflict resolution signal generator
//!
//! This module generates signals for conflict resolution sessions when
//! progressive merge detects conflicts. Unlike regular merge signals that
//! are spawned for auto-merge conflicts, these signals are specifically
//! for stages in the MergeConflict status.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::types::MergeConflictSignalContent;

/// Generate a signal file for a merge conflict resolution session.
///
/// This signal is generated when a stage transitions to MergeConflict status
/// after progressive merge detects conflicts. The session runs in the main
/// repository to resolve conflicts.
///
/// Unlike regular stage signals, this includes:
/// - Original stage context (description, files modified)
/// - List of conflicting files
/// - Instructions for conflict resolution
/// - Command to signal completion
pub fn generate_merge_conflict_signal(
    session: &Session,
    stage: &Stage,
    merge_point: &str,
    conflicting_files: &[String],
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    let signal_path = signals_dir.join(format!("{}.md", session.id));
    let content =
        format_merge_conflict_signal_content(session, stage, merge_point, conflicting_files);

    fs::write(&signal_path, &content).with_context(|| {
        format!(
            "Failed to write merge conflict signal file: {}",
            signal_path.display()
        )
    })?;

    Ok(signal_path)
}

/// Read and parse a merge conflict signal file.
///
/// Returns `None` if the signal file doesn't exist or isn't a merge conflict signal.
pub fn read_merge_conflict_signal(
    session_id: &str,
    work_dir: &Path,
) -> Result<Option<MergeConflictSignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    // Check if this is a merge conflict signal by looking for the specific header
    if !content.contains("# Merge Conflict Resolution:") {
        return Ok(None);
    }

    let parsed = parse_merge_conflict_signal_content(session_id, &content)?;
    Ok(Some(parsed))
}

pub(super) fn format_merge_conflict_signal_content(
    session: &Session,
    stage: &Stage,
    merge_point: &str,
    conflicting_files: &[String],
) -> String {
    let mut content = String::new();

    content.push_str(&format!("# Merge Conflict Resolution: {}\n\n", session.id));

    // Explain the situation
    content.push_str("## Situation\n\n");
    content.push_str(&format!(
        "Stage **'{}'** completed successfully but cannot merge to **'{}'**.\n\n",
        stage.id, merge_point
    ));
    content.push_str("You are in the main repository, not a worktree. Your task is to resolve\n");
    content.push_str("the merge conflicts and complete the merge.\n\n");

    // Conflicting files
    content.push_str("## Conflicting Files\n\n");
    if conflicting_files.is_empty() {
        content.push_str("_Run `git status` to see current conflicts_\n");
    } else {
        for file in conflicting_files {
            content.push_str(&format!("- `{file}`\n"));
        }
    }
    content.push('\n');

    // Task instructions
    content.push_str("## Your Task\n\n");
    let source_branch = format!("loom/{}", stage.id);
    content.push_str(&format!(
        "1. If not in merge state, run: `git merge {source_branch}`\n"
    ));
    content.push_str("2. Resolve conflicts in the listed files\n");
    content.push_str("3. Stage resolved files: `git add <files>`\n");
    content.push_str("4. Complete merge: `git commit`\n");
    content.push_str(&format!(
        "5. Signal completion: `loom stage merge-complete {}`\n\n",
        stage.id
    ));

    // Stage context
    content.push_str("## Context\n\n");
    content.push_str(&format!("**Stage**: {} ({})\n\n", stage.name, stage.id));
    if let Some(desc) = &stage.description {
        content.push_str(&format!("**Description**:\n{desc}\n\n"));
    }

    // Files the stage was working on (helps understand the changes)
    if !stage.files.is_empty() {
        content.push_str("**Files touched by this stage**:\n");
        for file in &stage.files {
            content.push_str(&format!("- `{file}`\n"));
        }
        content.push('\n');
    }

    // Target information
    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    content.push_str(&format!("- **Source Branch**: loom/{}\n", stage.id));
    content.push_str(&format!("- **Target Branch**: {merge_point}\n\n"));

    // Execution rules
    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` rules. Key reminders:\n\n");
    content.push_str("- **Preserve intent from BOTH branches** where possible\n");
    content.push_str("- **Do NOT modify code** beyond what's needed for conflict resolution\n");
    content.push_str("- **Ask the user** if unclear how to resolve a conflict\n");
    content.push_str("- **Use TodoWrite** to track resolution progress\n");

    content
}

pub(super) fn parse_merge_conflict_signal_content(
    session_id: &str,
    content: &str,
) -> Result<MergeConflictSignalContent> {
    let mut stage_id = String::new();
    let mut merge_point = String::new();
    let mut conflicting_files = Vec::new();

    let mut current_section = "";

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("## ") {
            current_section = trimmed.trim_start_matches("## ");
            continue;
        }

        match current_section {
            "Target" => {
                if let Some(id) = trimmed.strip_prefix("- **Stage**: ") {
                    stage_id = id.to_string();
                } else if let Some(branch) = trimmed.strip_prefix("- **Target Branch**: ") {
                    merge_point = branch.to_string();
                }
            }
            "Conflicting Files" => {
                if let Some(file) = trimmed.strip_prefix("- `") {
                    if let Some(f) = file.strip_suffix('`') {
                        conflicting_files.push(f.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    if stage_id.is_empty() {
        bail!("Merge conflict signal file is missing stage_id");
    }

    Ok(MergeConflictSignalContent {
        session_id: session_id.to_string(),
        stage_id,
        merge_point,
        conflicting_files,
    })
}
