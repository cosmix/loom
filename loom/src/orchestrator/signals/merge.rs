use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::helpers;
use super::types::MergeSignalContent;

/// Generate a signal file for a merge conflict resolution session.
///
/// Unlike regular stage signals that run in worktrees, merge signals direct
/// the session to work in the main repository to resolve merge conflicts.
pub fn generate_merge_signal(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
    work_dir: &Path,
) -> Result<PathBuf> {
    let content = format_merge_signal_content(
        session,
        stage,
        source_branch,
        target_branch,
        conflicting_files,
    );
    helpers::write_signal_file(&session.id, &content, work_dir)
}

/// Read and parse a merge signal file.
///
/// Returns `None` if the signal file doesn't exist or isn't a merge signal.
pub fn read_merge_signal(session_id: &str, work_dir: &Path) -> Result<Option<MergeSignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    // Check if this is a merge signal by looking for the merge-specific header
    if !content.contains("# Merge Signal:") {
        return Ok(None);
    }

    let parsed = parse_merge_signal_content(session_id, &content)?;
    Ok(Some(parsed))
}

pub(super) fn format_merge_signal_content(
    session: &Session,
    stage: &Stage,
    source_branch: &str,
    target_branch: &str,
    conflicting_files: &[String],
) -> String {
    let mut content = String::new();

    content.push_str(&format!("# Merge Signal: {}\n\n", session.id));

    // Merge context - explain the situation
    content.push_str("## Merge Context\n\n");
    content.push_str("You are resolving a **merge conflict** in the main repository.\n\n");
    content.push_str("- This is NOT a regular stage execution - you are fixing conflicts\n");
    content.push_str("- Work directly in the main repository (not a worktree)\n");
    content.push_str("- Follow the merge instructions below carefully\n\n");

    // Execution rules for merge sessions
    content.push_str(&helpers::format_execution_rules_section("BOTH branches"));

    // Target information
    content.push_str(&helpers::format_target_section(
        &session.id,
        &stage.id,
        Some(source_branch),
        target_branch,
    ));

    // Stage context (if available)
    content.push_str(&helpers::format_stage_context_section(stage));

    // Conflicting files
    content.push_str(&helpers::format_conflicting_files_section(
        conflicting_files,
    ));

    // Task instructions
    content.push_str("## Your Task\n\n");
    content.push_str(&format!(
        "1. Run: `git merge {source_branch}` (if not already in merge state)\n"
    ));
    content.push_str("2. Resolve conflicts in the files listed above\n");
    content.push_str("3. Stage resolved files: `git add <resolved-files>`\n");
    content.push_str("4. Review changes and complete the merge: `git commit`\n");
    content.push_str(&format!(
        "5. Run: `loom stage merge {} --resolved`\n",
        stage.id
    ));
    content.push_str(&format!(
        "6. Clean up worktree and branch: `loom worktree remove {}`\n\n",
        stage.id
    ));

    // Important notes
    content.push_str("## Important\n\n");
    content.push_str("- Do NOT modify code beyond what's needed for conflict resolution\n");
    content.push_str("- Preserve intent from BOTH branches where possible\n");
    content.push_str("- If unclear how to resolve, ask the user for guidance\n");
    content.push_str(&format!(
        "- **After completing the merge commit**, run `loom worktree remove {}` to clean up\n",
        stage.id
    ));

    content
}

pub(super) fn parse_merge_signal_content(
    session_id: &str,
    content: &str,
) -> Result<MergeSignalContent> {
    let sections = helpers::parse_signal_sections(content);

    // Extract from "Target" section
    let target_lines = sections
        .get("Target")
        .map(|v| v.as_slice())
        .unwrap_or_default();
    let stage_id = helpers::extract_field_from_lines(target_lines, "Stage")
        .unwrap_or_default()
        .to_string();
    let source_branch = helpers::extract_field_from_lines(target_lines, "Source Branch")
        .unwrap_or_default()
        .to_string();
    let target_branch = helpers::extract_field_from_lines(target_lines, "Target Branch")
        .unwrap_or_default()
        .to_string();

    // Extract from "Conflicting Files" section
    let conflict_lines = sections
        .get("Conflicting Files")
        .map(|v| v.as_slice())
        .unwrap_or_default();
    let conflicting_files = helpers::extract_backtick_items(conflict_lines);

    if stage_id.is_empty() {
        bail!("Merge signal file is missing stage_id");
    }

    Ok(MergeSignalContent {
        session_id: session_id.to_string(),
        stage_id,
        source_branch,
        target_branch,
        conflicting_files,
    })
}
