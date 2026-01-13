//! Base branch conflict resolution signal generation
//!
//! When a stage has multiple dependencies, loom creates a base branch (loom/_base/{stage_id})
//! by merging all dependency branches. If this merge fails due to conflicts, we spawn a
//! Claude Code session to resolve them before the stage can proceed.

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::types::BaseConflictSignalContent;

/// Generate a signal file for a base branch conflict resolution session.
///
/// Unlike regular stage signals that run in worktrees, base conflict signals direct
/// the session to work in the main repository to resolve conflicts between dependency
/// branches before the stage can start.
pub fn generate_base_conflict_signal(
    session: &Session,
    stage: &Stage,
    source_branches: &[String],
    target_branch: &str,
    conflicting_files: &[String],
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    let signal_path = signals_dir.join(format!("{}.md", session.id));
    let content = format_base_conflict_signal_content(
        session,
        stage,
        source_branches,
        target_branch,
        conflicting_files,
    );

    fs::write(&signal_path, &content).with_context(|| {
        format!(
            "Failed to write base conflict signal file: {}",
            signal_path.display()
        )
    })?;

    Ok(signal_path)
}

/// Read and parse a base conflict signal file.
///
/// Returns `None` if the signal file doesn't exist or isn't a base conflict signal.
pub fn read_base_conflict_signal(
    session_id: &str,
    work_dir: &Path,
) -> Result<Option<BaseConflictSignalContent>> {
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));

    if !signal_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&signal_path).context("Failed to read signal file")?;

    // Check if this is a base conflict signal by looking for the specific header
    if !content.contains("# Base Conflict Signal:") {
        return Ok(None);
    }

    let parsed = parse_base_conflict_signal_content(session_id, &content)?;
    Ok(Some(parsed))
}

fn format_base_conflict_signal_content(
    session: &Session,
    stage: &Stage,
    source_branches: &[String],
    target_branch: &str,
    conflicting_files: &[String],
) -> String {
    let mut content = String::new();

    content.push_str(&format!("# Base Conflict Signal: {}\n\n", session.id));

    // Context - explain the situation
    content.push_str("## Context\n\n");
    content
        .push_str("You are resolving a **base branch merge conflict** in the main repository.\n\n");
    content.push_str(&format!(
        "Stage `{}` depends on multiple completed stages. Before it can start, loom must merge \
         all dependency branches into a base branch (`{}`). This merge has conflicts.\n\n",
        stage.id, target_branch
    ));
    content.push_str("- This is NOT a regular stage execution - you are fixing conflicts\n");
    content.push_str("- Work directly in the main repository (not a worktree)\n");
    content.push_str("- After resolving, the user runs `loom retry {stage_id}` to continue\n\n");

    // Execution rules
    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` rules. Key reminders:\n");
    content.push_str("- **Do NOT modify code** beyond what's needed for conflict resolution\n");
    content.push_str("- **Preserve intent from ALL branches** where possible\n");
    content.push_str("- **Ask the user** if unclear how to resolve a conflict\n");
    content.push_str("- **Use TodoWrite** to track resolution progress\n\n");

    // Target information
    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {}\n", session.id));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    content.push_str(&format!("- **Target Branch**: {target_branch}\n"));
    content.push('\n');

    // Source branches
    content.push_str("## Source Branches\n\n");
    content.push_str("These dependency branches are being merged:\n\n");
    for branch in source_branches {
        content.push_str(&format!("- `{branch}`\n"));
    }
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
    content.push_str("1. Check merge status: `git status`\n");
    content.push_str("2. Resolve conflicts in the files listed above\n");
    content.push_str("3. Stage resolved files: `git add <resolved-files>`\n");
    content.push_str("4. Complete the merge: `git commit`\n");
    content.push_str(&format!(
        "5. Inform the user to run: `loom retry {}`\n\n",
        stage.id
    ));

    // Important notes
    content.push_str("## Important\n\n");
    content.push_str("- Do NOT modify code beyond what's needed for conflict resolution\n");
    content.push_str("- Preserve intent from ALL branches where possible\n");
    content.push_str("- If unclear how to resolve, ask the user for guidance\n");
    content.push_str(&format!(
        "- After completing the merge, tell the user to run `loom retry {}`\n",
        stage.id
    ));

    content
}

fn parse_base_conflict_signal_content(
    session_id: &str,
    content: &str,
) -> Result<BaseConflictSignalContent> {
    let mut stage_id = String::new();
    let mut target_branch = String::new();
    let mut source_branches = Vec::new();
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
                    target_branch = branch.to_string();
                }
            }
            "Source Branches" => {
                if let Some(branch) = trimmed.strip_prefix("- `") {
                    if let Some(b) = branch.strip_suffix('`') {
                        source_branches.push(b.to_string());
                    }
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
        bail!("Base conflict signal file is missing stage_id");
    }

    Ok(BaseConflictSignalContent {
        session_id: session_id.to_string(),
        stage_id,
        source_branches,
        target_branch,
        conflicting_files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{StageStatus, StageType};
    use chrono::Utc;

    fn create_test_stage(id: &str) -> Stage {
        Stage {
            id: id.to_string(),
            name: format!("Test Stage {id}"),
            description: Some("A test stage for conflict resolution".to_string()),
            status: StageStatus::WaitingForDeps,
            dependencies: vec!["dep-1".to_string(), "dep-2".to_string()],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            stage_type: StageType::default(),
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
            auto_merge: None,
            working_dir: Some(".".to_string()),
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
        }
    }

    #[test]
    fn test_format_base_conflict_signal() {
        let session = Session::new_base_conflict("loom/_base/test-stage".to_string());
        let stage = create_test_stage("test-stage");
        let source_branches = vec!["loom/dep-1".to_string(), "loom/dep-2".to_string()];
        let conflicting_files = vec!["src/lib.rs".to_string(), "src/main.rs".to_string()];

        let content = format_base_conflict_signal_content(
            &session,
            &stage,
            &source_branches,
            "loom/_base/test-stage",
            &conflicting_files,
        );

        assert!(content.contains("# Base Conflict Signal:"));
        assert!(content.contains("**Stage**: test-stage"));
        assert!(content.contains("**Target Branch**: loom/_base/test-stage"));
        assert!(content.contains("- `loom/dep-1`"));
        assert!(content.contains("- `loom/dep-2`"));
        assert!(content.contains("- `src/lib.rs`"));
        assert!(content.contains("- `src/main.rs`"));
        assert!(content.contains("loom retry test-stage"));
    }

    #[test]
    fn test_parse_base_conflict_signal() {
        let session = Session::new_base_conflict("loom/_base/my-stage".to_string());
        let stage = create_test_stage("my-stage");
        let source_branches = vec!["loom/a".to_string(), "loom/b".to_string()];
        let conflicting_files = vec!["file.rs".to_string()];

        let content = format_base_conflict_signal_content(
            &session,
            &stage,
            &source_branches,
            "loom/_base/my-stage",
            &conflicting_files,
        );

        let parsed = parse_base_conflict_signal_content(&session.id, &content).unwrap();

        assert_eq!(parsed.session_id, session.id);
        assert_eq!(parsed.stage_id, "my-stage");
        assert_eq!(parsed.target_branch, "loom/_base/my-stage");
        assert_eq!(parsed.source_branches, source_branches);
        assert_eq!(parsed.conflicting_files, conflicting_files);
    }
}
