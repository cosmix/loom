//! Merge conflict resolution signal generator
//!
//! This module generates signals for conflict resolution sessions when
//! progressive merge detects conflicts. Unlike regular merge signals that
//! are spawned for auto-merge conflicts, these signals are specifically
//! for stages in the MergeConflict status.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::git::branch::branch_name_for_stage;
use crate::models::session::Session;
use crate::models::stage::Stage;

use super::helpers;
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
    let content =
        format_merge_conflict_signal_content(session, stage, merge_point, conflicting_files);
    helpers::write_signal_file(&session.id, &content, work_dir)
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
    content.push_str(&helpers::format_conflicting_files_section(
        conflicting_files,
    ));

    // Task instructions
    content.push_str("## Your Task\n\n");
    let source_branch = branch_name_for_stage(&stage.id);
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
    content.push_str(&helpers::format_target_section(
        &session.id,
        &stage.id,
        Some(&branch_name_for_stage(&stage.id)),
        merge_point,
    ));

    // Execution rules
    content.push_str(&helpers::format_execution_rules_section("BOTH branches"));

    content
}

pub(super) fn parse_merge_conflict_signal_content(
    session_id: &str,
    content: &str,
) -> Result<MergeConflictSignalContent> {
    let sections = helpers::parse_signal_sections(content);

    // Extract from "Target" section
    let target_lines = sections
        .get("Target")
        .map(|v| v.as_slice())
        .unwrap_or_default();
    let stage_id = helpers::extract_field_from_lines(target_lines, "Stage")
        .unwrap_or_default()
        .to_string();
    let merge_point = helpers::extract_field_from_lines(target_lines, "Target Branch")
        .unwrap_or_default()
        .to_string();

    // Extract from "Conflicting Files" section
    let conflict_lines = sections
        .get("Conflicting Files")
        .map(|v| v.as_slice())
        .unwrap_or_default();
    let conflicting_files = helpers::extract_backtick_items(conflict_lines);

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
