use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::session::Session;
use crate::models::stage::Stage;

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
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    let signal_path = signals_dir.join(format!("{}.md", session.id));
    let content = format_merge_signal_content(
        session,
        stage,
        source_branch,
        target_branch,
        conflicting_files,
    );

    fs::write(&signal_path, &content).with_context(|| {
        format!(
            "Failed to write merge signal file: {}",
            signal_path.display()
        )
    })?;

    Ok(signal_path)
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
    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` rules. Key reminders:\n");
    content.push_str("- **Do NOT modify code** beyond what's needed for conflict resolution\n");
    content.push_str("- **Preserve intent from BOTH branches** where possible\n");
    content.push_str("- **Ask the user** if unclear how to resolve a conflict\n");
    content.push_str("- **Use TodoWrite** to track resolution progress\n\n");

    // Target information
    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    content.push_str(&format!("- **Source Branch**: {source_branch}\n"));
    content.push_str(&format!("- **Target Branch**: {target_branch}\n"));
    content.push('\n');

    // Stage context (if available)
    if let Some(desc) = &stage.description {
        content.push_str("## Stage Context\n\n");
        content.push_str(&format!("**{0}**: {1}\n\n", stage.name, desc));
    }

    // Conflicting files
    content.push_str("## Conflicting Files\n\n");
    if conflicting_files.is_empty() {
        content
            .push_str("_No specific files listed - run `git status` to see current conflicts_\n");
    } else {
        for file in conflicting_files {
            content.push_str(&format!("- `{file}`\n"));
        }
    }
    content.push('\n');

    // Task instructions
    content.push_str("## Your Task\n\n");
    content.push_str(&format!(
        "1. Run: `git merge {source_branch}` (if not already in merge state)\n"
    ));
    content.push_str("2. Resolve conflicts in the files listed above\n");
    content.push_str("3. Stage resolved files: `git add <resolved-files>`\n");
    content.push_str("4. Review changes and complete the merge: `git commit`\n\n");

    // Important notes
    content.push_str("## Important\n\n");
    content.push_str("- Do NOT modify code beyond what's needed for conflict resolution\n");
    content.push_str("- Preserve intent from BOTH branches where possible\n");
    content.push_str("- If unclear how to resolve, ask the user for guidance\n");
    content.push_str("- After completing the merge, loom will automatically detect and clean up\n");

    content
}

pub(super) fn parse_merge_signal_content(session_id: &str, content: &str) -> Result<MergeSignalContent> {
    let mut stage_id = String::new();
    let mut source_branch = String::new();
    let mut target_branch = String::new();
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
                } else if let Some(branch) = trimmed.strip_prefix("- **Source Branch**: ") {
                    source_branch = branch.to_string();
                } else if let Some(branch) = trimmed.strip_prefix("- **Target Branch**: ") {
                    target_branch = branch.to_string();
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
