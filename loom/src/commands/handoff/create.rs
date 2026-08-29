use anyhow::{Context, Result};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::commands::common::find_work_dir;
use crate::git::branch::current_branch;
use crate::handoff::generator::{generate_handoff, HandoffContent};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::stage_context_tokens;
use crate::verify::transitions::{load_stage, update_stage};

/// The `--trigger` value an agent uses when its own context ceiling forced the
/// handoff. Only this trigger ends the session's turn on the stage; every other
/// trigger just writes a document.
const CEILING_TRIGGER: &str = "ceiling";

/// Execute the `loom handoff` command
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
    if let Ok(branch) = current_branch(&std::env::current_dir()?) {
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
        trigger
    );
    let goals_with_trigger = format!("{}{}", content.goals, trigger_note);
    content = content.with_goals(goals_with_trigger);

    // The heartbeat file is the only place a real context reading exists: the
    // hooks measure the transcript and write it there. Before this was read,
    // every handoff loom wrote recorded a context of zero.
    let context_tokens = stage_context_tokens(&work_dir, &stage_id).unwrap_or(0);
    content = content.with_context_tokens(context_tokens);

    let mut session = Session::new();
    session.id = session_id.clone();
    session.stage_id = Some(stage_id.clone());
    session.status = SessionStatus::Running;
    session.context_tokens = context_tokens;

    // Generate the handoff file
    let handoff_path = generate_handoff(&session, &stage, content, &work_dir)?;

    if trigger == CEILING_TRIGGER {
        end_turn_for_handoff(&stage_id, &work_dir);
    }

    // Print the handoff file path (hooks parse this output)
    println!("{}", handoff_path.display());

    Ok(())
}

/// Mark the stage `NeedsHandoff` so the daemon takes the session down and
/// spawns a successor with this handoff inlined.
///
/// Only meaningful while the stage is `Executing`: a stage that already moved
/// on has an authority of its own, and a CLI-side write must not overrule it.
/// Best-effort by design — the handoff document is already on disk, and losing
/// the transition costs a continuation, not the work.
fn end_turn_for_handoff(stage_id: &str, work_dir: &Path) {
    let mut skipped = None;
    let result = update_stage(stage_id, work_dir, |stage| {
        if stage.status != StageStatus::Executing {
            skipped = Some(stage.status.clone());
            return Ok(());
        }
        stage.try_mark_needs_handoff()
    });

    match (result, skipped) {
        (Err(e), _) => eprintln!("Warning: could not mark stage '{stage_id}' NeedsHandoff: {e}"),
        (Ok(_), Some(status)) => eprintln!(
            "Note: Stage '{stage_id}' is {status:?}, not executing. Skipping handoff transition."
        ),
        (Ok(_), None) => eprintln!("Stage '{stage_id}' marked NeedsHandoff; session will end."),
    }
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
    use serial_test::serial;

    #[test]
    fn test_resolve_stage_id_from_arg() {
        let stage_arg = Some("test-stage".to_string());
        let result = resolve_stage_id(&stage_arg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-stage");
    }

    #[test]
    #[serial]
    fn test_resolve_stage_id_from_env() {
        let original = env::var("LOOM_STAGE_ID").ok();
        env::set_var("LOOM_STAGE_ID", "env-stage");
        let stage_arg = None;
        let result = resolve_stage_id(&stage_arg);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "env-stage");
        // Restore original value
        match original {
            Some(val) => env::set_var("LOOM_STAGE_ID", val),
            None => env::remove_var("LOOM_STAGE_ID"),
        }
    }

    #[test]
    #[serial]
    fn test_resolve_stage_id_missing() {
        let original = env::var("LOOM_STAGE_ID").ok();
        env::remove_var("LOOM_STAGE_ID");
        let stage_arg = None;
        let result = resolve_stage_id(&stage_arg);
        assert!(result.is_err());
        // Restore original value
        if let Some(val) = original {
            env::set_var("LOOM_STAGE_ID", val);
        }
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

    /// `--trigger ceiling` is the agent saying "I am out of room": it must end
    /// the turn, not just leave a document behind. Without the transition the
    /// daemon never kills the session, and the stage sits Executing behind an
    /// agent that has already stopped working.
    #[test]
    fn ceiling_trigger_marks_an_executing_stage_needing_handoff() {
        use crate::verify::transitions::create_stage;

        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let mut stage = Stage::new("ceiling".to_string(), None);
        stage.id = "ceiling".to_string();
        stage.status = StageStatus::Executing;
        create_stage(&stage, work_dir).unwrap();

        end_turn_for_handoff("ceiling", work_dir);

        let reloaded = load_stage("ceiling", work_dir).unwrap();
        assert_eq!(reloaded.status, StageStatus::NeedsHandoff);
    }

    /// A stage that already moved on has an authority of its own; a late
    /// CLI-side write must not drag it back out of a terminal state.
    #[test]
    fn ceiling_trigger_leaves_a_stage_that_already_moved_on() {
        use crate::verify::transitions::create_stage;

        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let mut stage = Stage::new("done".to_string(), None);
        stage.id = "done".to_string();
        stage.status = StageStatus::Completed;
        create_stage(&stage, work_dir).unwrap();

        end_turn_for_handoff("done", work_dir);

        let reloaded = load_stage("done", work_dir).unwrap();
        assert_eq!(reloaded.status, StageStatus::Completed);
    }
}
