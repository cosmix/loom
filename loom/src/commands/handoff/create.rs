use anyhow::{Context, Result};
use chrono::Utc;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::handoff::generator::{generate_handoff, HandoffContent};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::verify::transitions::load_stage;

/// Execute the `loom handoff create` command
///
/// Creates a handoff file capturing current session state for context recovery.
///
/// # Arguments
/// * `stage_arg` - Optional stage ID from CLI (uses LOOM_STAGE_ID env var if not provided)
/// * `session_arg` - Optional session ID from CLI (uses LOOM_SESSION_ID env var if not provided)
/// * `trigger` - Trigger type (e.g., "manual", "precompact", "session_end")
/// * `message` - Optional message to include in the handoff
pub fn execute(
    stage_arg: Option<String>,
    session_arg: Option<String>,
    trigger: String,
    message: Option<String>,
) -> Result<()> {
    // Resolve stage and session IDs from arguments or environment
    let stage_id = resolve_stage_id(&stage_arg)?;
    let session_id = resolve_session_id(&session_arg)?;

    // Determine work directory (look for .work in current dir or as symlink)
    let work_dir = find_work_dir()?;

    // Load stage (gracefully handle missing stage)
    let stage = load_stage(&stage_id, &work_dir).unwrap_or_else(|_| {
        // Create minimal stage if loading fails
        Stage {
            id: stage_id.clone(),
            name: stage_id.clone(),
            description: None,
            plan_id: None,
            ..Default::default()
        }
    });

    // Build handoff content
    let mut content = HandoffContent::new(session_id.clone(), stage_id.clone());

    // Add plan ID if available
    if let Some(ref plan_id) = stage.plan_id {
        content = content.with_plan_id(Some(plan_id.to_string()));
    }

    // Add goals from stage description if available
    if let Some(ref description) = stage.description {
        content = content.with_goals(description.to_string());
    }

    // Get current branch
    if let Ok(branch) = get_current_branch() {
        content = content.with_current_branch(Some(branch));
    }

    // Get modified files from git status
    if let Ok(files) = get_modified_files() {
        content = content.with_files_modified(files);
    }

    // Read session memory if available
    let memory_path = work_dir.join("memory").join(format!("{}.md", session_id));
    if memory_path.exists() {
        if let Ok(memory_content) = fs::read_to_string(&memory_path) {
            content = content.with_memory_content(Some(memory_content));
        }
    }

    // Add message as a next step if provided
    if let Some(msg) = &message {
        content = content.with_next_steps(vec![msg.clone()]);
    }

    // Add trigger information to goals
    let trigger_note = format!(
        "\n\nHandoff created via: {} (trigger: {})",
        if stage_arg.is_some() || session_arg.is_some() {
            "manual CLI"
        } else {
            "environment"
        },
        &trigger
    );
    let goals_with_trigger = format!("{}{}", content.goals, trigger_note);
    content = content.with_goals(goals_with_trigger);

    // Create a minimal Session for generate_handoff (the _session parameter is unused)
    let session = Session {
        id: session_id.clone(),
        stage_id: Some(stage_id.clone()),
        status: SessionStatus::Running,
        context_tokens: 0,
        context_limit: 0,
        created_at: Utc::now(),
        last_active: Utc::now(),
        worktree_path: None,
        pid: None,
        session_type: Default::default(),
        merge_source_branch: None,
        merge_target_branch: None,
    };

    // Generate the handoff file
    let handoff_path = generate_handoff(&session, &stage, content, &work_dir)?;

    // Print the handoff file path (hooks parse this output)
    println!("{}", handoff_path.display());

    Ok(())
}

/// Resolve stage ID from argument or LOOM_STAGE_ID environment variable
fn resolve_stage_id(stage_arg: &Option<String>) -> Result<String> {
    if let Some(stage) = stage_arg {
        return Ok(stage.clone());
    }

    env::var("LOOM_STAGE_ID").context(
        "No stage ID provided and LOOM_STAGE_ID environment variable not set. \
         Use --stage <ID> or run from a loom session.",
    )
}

/// Resolve session ID from argument or LOOM_SESSION_ID environment variable
fn resolve_session_id(session_arg: &Option<String>) -> Result<String> {
    if let Some(session) = session_arg {
        return Ok(session.clone());
    }

    env::var("LOOM_SESSION_ID").context(
        "No session ID provided and LOOM_SESSION_ID environment variable not set. \
         Use --session <ID> or run from a loom session.",
    )
}

/// Find the .work directory (either as a real directory or symlink)
fn find_work_dir() -> Result<PathBuf> {
    let work_path = PathBuf::from(".work");

    if work_path.exists() {
        Ok(work_path)
    } else {
        anyhow::bail!(
            "No .work directory found. This command must be run from a loom worktree or project root."
        )
    }
}

/// Get the current git branch name
fn get_current_branch() -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("Failed to run git rev-parse")?;

    if output.status.success() {
        let branch = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in git output")?
            .trim()
            .to_string();
        Ok(branch)
    } else {
        anyhow::bail!("Failed to get current branch")
    }
}

/// Get list of modified files from git status
fn get_modified_files() -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["status", "--short"])
        .output()
        .context("Failed to run git status")?;

    if output.status.success() {
        let status_output =
            String::from_utf8(output.stdout).context("Invalid UTF-8 in git output")?;

        let files: Vec<String> = status_output
            .lines()
            .filter_map(|line| {
                // Git status --short format: "XY filename"
                // Where X is staged status, Y is unstaged status
                if line.len() >= 3 {
                    Some(line[3..].trim().to_string())
                } else {
                    None
                }
            })
            .collect();

        Ok(files)
    } else {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_stage_id_from_arg() {
        let stage_arg = Some("test-stage".to_string());
        let result = resolve_stage_id(&stage_arg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-stage");
    }

    #[test]
    fn test_resolve_stage_id_from_env() {
        env::set_var("LOOM_STAGE_ID", "env-stage");
        let stage_arg = None;
        let result = resolve_stage_id(&stage_arg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "env-stage");
        env::remove_var("LOOM_STAGE_ID");
    }

    #[test]
    fn test_resolve_stage_id_missing() {
        env::remove_var("LOOM_STAGE_ID");
        let stage_arg = None;
        let result = resolve_stage_id(&stage_arg);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_session_id_from_arg() {
        let session_arg = Some("test-session".to_string());
        let result = resolve_session_id(&session_arg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-session");
    }

    #[test]
    fn test_build_handoff_content() {
        let session_id = "test-session".to_string();
        let stage_id = "test-stage".to_string();

        let content = HandoffContent::new(session_id.clone(), stage_id.clone())
            .with_goals("Test goals".to_string())
            .with_current_branch(Some("main".to_string()))
            .with_files_modified(vec!["file1.rs".to_string(), "file2.rs".to_string()]);

        assert_eq!(content.session_id, session_id);
        assert_eq!(content.stage_id, stage_id);
        assert_eq!(content.goals, "Test goals");
        assert_eq!(content.current_branch, Some("main".to_string()));
        assert_eq!(content.files_modified.len(), 2);
    }
}
