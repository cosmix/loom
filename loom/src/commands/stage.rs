//! Stage state manipulation
//! Usage: loom stage <id> [complete|block|reset|ready]

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::models::session::{Session, SessionStatus};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, save_stage, trigger_dependents};

/// Mark a stage as complete, optionally running acceptance criteria.
/// If acceptance criteria pass, auto-verifies the stage and triggers dependents.
/// If --no-verify is used or criteria fail, marks as Completed for manual review.
pub fn complete(stage_id: String, session_id: Option<String>, no_verify: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Resolve session_id: CLI arg > stage.session field > scan sessions directory
    let session_id = session_id
        .or_else(|| stage.session.clone())
        .or_else(|| find_session_for_stage(&stage_id, work_dir));

    // Resolve worktree path from stage's worktree field
    let working_dir: Option<PathBuf> = stage
        .worktree
        .as_ref()
        .map(|w| PathBuf::from(".worktrees").join(w))
        .filter(|p| p.exists());

    // Track whether all acceptance criteria passed
    let mut all_passed = true;

    // Run acceptance criteria unless --no-verify is specified
    if !no_verify && !stage.acceptance.is_empty() {
        println!("Running acceptance criteria for stage '{stage_id}'...");
        if let Some(ref dir) = working_dir {
            println!("  (working directory: {})", dir.display());
        }

        for criterion in &stage.acceptance {
            println!("  → {criterion}");
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(criterion);

            if let Some(ref dir) = working_dir {
                cmd.current_dir(dir);
            }

            let status = cmd
                .status()
                .with_context(|| format!("Failed to run: {criterion}"))?;

            if !status.success() {
                all_passed = false;
                println!("  ✗ FAILED: {criterion}");
                break;
            }
            println!("  ✓ passed");
        }

        if all_passed {
            println!("All acceptance criteria passed!");
        }
    } else if no_verify {
        // --no-verify means we skip criteria, so don't auto-verify
        all_passed = false;
    } else {
        // No acceptance criteria defined - treat as passed
        all_passed = true;
    }

    // Always try to kill the tmux session for this stage (even without session_id)
    cleanup_tmux_for_stage(&stage_id);

    // Cleanup session resources (update session status, remove signal)
    if let Some(ref sid) = session_id {
        cleanup_session_resources(&stage_id, sid, work_dir);
    }

    // Auto-verify if all criteria passed, otherwise mark as Completed
    if all_passed {
        // Must transition through Completed before Verified (state machine requirement)
        stage.try_complete(None)?;
        stage.try_mark_verified()?;
        save_stage(&stage, work_dir)?;
        println!("Stage '{stage_id}' verified!");

        // Trigger dependent stages
        let triggered = trigger_dependents(&stage_id, work_dir)
            .context("Failed to trigger dependent stages")?;

        if !triggered.is_empty() {
            println!("Triggered {} dependent stage(s):", triggered.len());
            for dep_id in &triggered {
                println!("  → {dep_id}");
            }
        }
    } else {
        stage.try_complete(None)?;
        save_stage(&stage, work_dir)?;
        println!("Stage '{stage_id}' marked as completed (needs manual verification)");
    }

    Ok(())
}

/// Kill tmux session for a stage (best-effort, doesn't require session_id)
fn cleanup_tmux_for_stage(stage_id: &str) {
    let tmux_name = format!("loom-{stage_id}");
    match Command::new("tmux")
        .args(["kill-session", "-t", &tmux_name])
        .output()
    {
        Ok(output) if output.status.success() => {
            println!("Killed tmux session '{tmux_name}'");
        }
        Ok(_) => {
            // Session may not exist or already dead - this is fine
        }
        Err(e) => {
            eprintln!("Warning: failed to kill tmux session '{tmux_name}': {e}");
        }
    }
}

/// Find session ID for a stage by scanning .work/sessions/
fn find_session_for_stage(stage_id: &str, work_dir: &Path) -> Option<String> {
    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.exists() {
        return None;
    }

    let entries = fs::read_dir(&sessions_dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        // Try to read and parse session file
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(session) = session_from_markdown(&content) {
                if session.stage_id.as_deref() == Some(stage_id) {
                    return Some(session.id);
                }
            }
        }
    }
    None
}

/// Clean up resources associated with a completed stage
///
/// This function performs best-effort cleanup and logs warnings on failure:
/// 1. Updates session status to Completed
/// 2. Removes the signal file
fn cleanup_session_resources(_stage_id: &str, session_id: &str, work_dir: &Path) {
    // 1. Update session status to Completed
    if let Err(e) = update_session_status(work_dir, session_id, SessionStatus::Completed) {
        eprintln!("Warning: failed to update session status: {e}");
    }

    // 2. Remove signal file
    let signal_path = work_dir.join("signals").join(format!("{session_id}.md"));
    match fs::remove_file(&signal_path) {
        Ok(()) => {
            println!("Removed signal file '{}'", signal_path.display());
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Signal file may not exist - this is fine
        }
        Err(e) => {
            eprintln!(
                "Warning: failed to remove signal file '{}': {e}",
                signal_path.display()
            );
        }
    }
}

/// Update a session's status in .work/sessions/
fn update_session_status(work_dir: &Path, session_id: &str, status: SessionStatus) -> Result<()> {
    let sessions_dir = work_dir.join("sessions");
    let session_path = sessions_dir.join(format!("{session_id}.md"));

    if !session_path.exists() {
        bail!("Session file not found: {}", session_path.display());
    }

    let content = fs::read_to_string(&session_path)
        .with_context(|| format!("Failed to read session file: {}", session_path.display()))?;

    // Parse session from markdown
    let session = session_from_markdown(&content)?;

    // Update status
    let mut session = session;
    session.status = status;
    session.last_active = chrono::Utc::now();

    // Write back
    let updated_content = session_to_markdown(&session);
    fs::write(&session_path, updated_content)
        .with_context(|| format!("Failed to write session file: {}", session_path.display()))?;

    Ok(())
}

/// Parse session from markdown with YAML frontmatter
fn session_from_markdown(content: &str) -> Result<Session> {
    let yaml_content = content
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---"))
        .map(|(yaml, _)| yaml)
        .ok_or_else(|| anyhow::anyhow!("Invalid session file format: missing frontmatter"))?;

    serde_yaml::from_str(yaml_content).context("Failed to parse session YAML")
}

/// Convert session to markdown format
fn session_to_markdown(session: &Session) -> String {
    let yaml = serde_yaml::to_string(session).unwrap_or_else(|_| String::from("{}"));

    format!(
        "---\n{yaml}---\n\n# Session: {}\n\n## Details\n\n- **Status**: {:?}\n- **Stage**: {}\n- **Tmux**: {}\n",
        session.id,
        session.status,
        session.stage_id.as_ref().unwrap_or(&"None".to_string()),
        session.tmux_session.as_ref().unwrap_or(&"None".to_string()),
    )
}

/// Block a stage with a reason
pub fn block(stage_id: String, reason: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.status = StageStatus::Blocked;
    stage.close_reason = Some(reason.clone());
    stage.updated_at = chrono::Utc::now();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' blocked");
    println!("Reason: {reason}");
    Ok(())
}

/// Reset a stage to pending
pub fn reset(stage_id: String, hard: bool, kill_session: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    // Kill tmux session if requested
    if kill_session {
        let tmux_name = format!("loom-{stage_id}");
        let _ = std::process::Command::new("tmux")
            .args(["kill-session", "-t", &tmux_name])
            .output();
    }

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.status = StageStatus::Pending;
    stage.completed_at = None;
    stage.close_reason = None;
    stage.updated_at = chrono::Utc::now();

    // Hard reset also clears session assignment
    if hard {
        stage.session = None;
    }

    save_stage(&stage, work_dir)?;

    let mode = if hard { "hard" } else { "soft" };
    println!("Stage '{stage_id}' reset to pending ({mode} reset)");
    Ok(())
}

/// Mark a stage as ready for execution
pub fn ready(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;
    stage.try_mark_ready()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' marked as ready");
    Ok(())
}

/// Mark a stage as waiting for user input (called by hooks)
pub fn waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently executing
    if stage.status != StageStatus::Executing {
        // Silently skip if not executing - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not executing. Skipping waiting transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.try_mark_waiting_for_input()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' waiting for user input");
    Ok(())
}

/// Resume a stage from waiting for input state (called by hooks)
pub fn resume_from_waiting(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Only transition if currently waiting for input
    if stage.status != StageStatus::WaitingForInput {
        // Silently skip if not waiting - hook may fire at wrong time
        eprintln!(
            "Note: Stage '{}' is {:?}, not waiting. Skipping resume transition.",
            stage_id, stage.status
        );
        return Ok(());
    }

    stage.try_mark_executing()?;
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' resumed execution");
    Ok(())
}

/// Hold a stage (prevent auto-execution even when ready)
pub fn hold(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if stage.held {
        println!("Stage '{stage_id}' is already held");
        return Ok(());
    }

    stage.hold();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' held");
    println!("The stage will not auto-execute. Use 'loom stage release {stage_id}' to unlock.");
    Ok(())
}

/// Release a held stage (allow auto-execution)
pub fn release(stage_id: String) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    if !stage.held {
        println!("Stage '{stage_id}' is not held");
        return Ok(());
    }

    stage.release();
    save_stage(&stage, work_dir)?;

    println!("Stage '{stage_id}' released");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::work_dir::WorkDir;
    use crate::models::stage::Stage;
    use chrono::Utc;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_stage(id: &str, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: format!("Stage {id}"),
            description: None,
            status,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
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
        }
    }

    fn setup_work_dir() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        work_dir.initialize().unwrap();
        temp_dir
    }

    fn save_test_stage(work_dir: &Path, stage: &Stage) {
        let yaml = serde_yaml::to_string(stage).unwrap();
        let content = format!("---\n{yaml}---\n\n# Stage: {}\n", stage.name);

        let stages_dir = work_dir.join("stages");
        fs::create_dir_all(&stages_dir).unwrap();

        let stage_path = stages_dir.join(format!("00-{}.md", stage.id));
        fs::write(stage_path, content).unwrap();
    }

    #[test]
    fn test_session_from_markdown_valid() {
        let content = r#"---
id: session-1
stage_id: stage-1
tmux_session: null
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session: session-1
"#;

        let result = session_from_markdown(content);

        assert!(result.is_ok());
        let session = result.unwrap();
        assert_eq!(session.id, "session-1");
        assert_eq!(session.stage_id, Some("stage-1".to_string()));
    }

    #[test]
    fn test_session_from_markdown_no_frontmatter() {
        let content = "No frontmatter here";

        let result = session_from_markdown(content);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing frontmatter"));
    }

    #[test]
    fn test_session_to_markdown() {
        let session = Session {
            id: "session-1".to_string(),
            stage_id: Some("stage-1".to_string()),
            tmux_session: Some("loom-stage-1".to_string()),
            worktree_path: None,
            pid: None,
            status: SessionStatus::Running,
            context_tokens: 0,
            context_limit: 200000,
            created_at: Utc::now(),
            last_active: Utc::now(),
        };

        let content = session_to_markdown(&session);

        assert!(content.starts_with("---\n"));
        assert!(content.contains("# Session: session-1"));
        assert!(content.contains("**Status**: Running"));
        assert!(content.contains("**Stage**: stage-1"));
    }

    #[test]
    fn test_find_session_for_stage_found() {
        let temp_dir = setup_work_dir();
        let work_dir = temp_dir.path().join(".work");

        let session_content = r#"---
id: session-1
stage_id: test-stage
tmux_session: null
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---

# Session
"#;

        let sessions_dir = work_dir.join("sessions");
        fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

        let result = find_session_for_stage("test-stage", &work_dir);

        assert_eq!(result, Some("session-1".to_string()));
    }

    #[test]
    fn test_find_session_for_stage_not_found() {
        let temp_dir = setup_work_dir();
        let work_dir = temp_dir.path().join(".work");

        let result = find_session_for_stage("nonexistent", &work_dir);

        assert_eq!(result, None);
    }

    #[test]
    fn test_find_session_for_stage_different_stage() {
        let temp_dir = setup_work_dir();
        let work_dir = temp_dir.path().join(".work");

        let session_content = r#"---
id: session-1
stage_id: other-stage
tmux_session: null
worktree_path: null
pid: null
status: running
context_tokens: 0
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T00:00:00Z"
---
"#;

        let sessions_dir = work_dir.join("sessions");
        fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

        let result = find_session_for_stage("test-stage", &work_dir);

        assert_eq!(result, None);
    }

    #[test]
    fn test_cleanup_tmux_for_stage_does_not_fail() {
        cleanup_tmux_for_stage("test-stage");
    }

    #[test]
    fn test_cleanup_session_resources() {
        let temp_dir = setup_work_dir();
        let work_dir = temp_dir.path().join(".work");

        let session = Session {
            id: "session-1".to_string(),
            stage_id: Some("test-stage".to_string()),
            tmux_session: None,
            worktree_path: None,
            pid: None,
            status: SessionStatus::Running,
            context_tokens: 0,
            context_limit: 200000,
            created_at: Utc::now(),
            last_active: Utc::now(),
        };

        let session_content = session_to_markdown(&session);
        let sessions_dir = work_dir.join("sessions");
        fs::write(sessions_dir.join("session-1.md"), session_content).unwrap();

        let signals_dir = work_dir.join("signals");
        fs::write(signals_dir.join("session-1.md"), "signal").unwrap();

        cleanup_session_resources("test-stage", "session-1", &work_dir);

        assert!(!signals_dir.join("session-1.md").exists());
    }

    #[test]
    #[serial]
    fn test_block_updates_status() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let stage = create_test_stage("test-stage", StageStatus::Ready);
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = block("test-stage".to_string(), "Test blocker".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok(), "block() failed: {:?}", result.err());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::Blocked);
        assert_eq!(loaded_stage.close_reason, Some("Test blocker".to_string()));
    }

    #[test]
    #[serial]
    fn test_reset_clears_completion() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let mut stage = create_test_stage("test-stage", StageStatus::Completed);
        stage.completed_at = Some(Utc::now());
        stage.close_reason = Some("Done".to_string());
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = reset("test-stage".to_string(), false, false);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok(), "reset() failed: {:?}", result.err());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::Pending);
        assert_eq!(loaded_stage.completed_at, None);
        assert_eq!(loaded_stage.close_reason, None);
    }

    #[test]
    #[serial]
    fn test_reset_hard_clears_session() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let mut stage = create_test_stage("test-stage", StageStatus::Executing);
        stage.session = Some("session-1".to_string());
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = reset("test-stage".to_string(), true, false);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.session, None);
    }

    #[test]
    #[serial]
    fn test_ready_marks_as_ready() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let stage = create_test_stage("test-stage", StageStatus::Pending);
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = ready("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::Ready);
    }

    #[test]
    #[serial]
    fn test_hold_sets_held_flag() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let stage = create_test_stage("test-stage", StageStatus::Ready);
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = hold("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert!(loaded_stage.held);
    }

    #[test]
    #[serial]
    fn test_release_clears_held_flag() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let mut stage = create_test_stage("test-stage", StageStatus::Ready);
        stage.held = true;
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = release("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert!(!loaded_stage.held);
    }

    #[test]
    #[serial]
    fn test_waiting_transitions_from_executing() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let stage = create_test_stage("test-stage", StageStatus::Executing);
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = waiting("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::WaitingForInput);
    }

    #[test]
    #[serial]
    fn test_waiting_skips_if_not_executing() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let stage = create_test_stage("test-stage", StageStatus::Ready);
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = waiting("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::Ready);
    }

    #[test]
    #[serial]
    fn test_resume_from_waiting_transitions_to_executing() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let stage = create_test_stage("test-stage", StageStatus::WaitingForInput);
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = resume_from_waiting("test-stage".to_string());

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::Executing);
    }

    #[test]
    #[serial]
    fn test_complete_with_passing_acceptance() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let mut stage = create_test_stage("test-stage", StageStatus::Executing);
        stage.acceptance = vec!["exit 0".to_string()];
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = complete("test-stage".to_string(), None, false);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok(), "complete() failed: {:?}", result.err());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::Verified);
    }

    #[test]
    #[serial]
    fn test_complete_with_no_verify_flag() {
        let temp_dir = setup_work_dir();
        let work_dir_path = temp_dir.path().join(".work");

        let mut stage = create_test_stage("test-stage", StageStatus::Executing);
        stage.acceptance = vec!["exit 1".to_string()];
        save_test_stage(&work_dir_path, &stage);

        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp_dir.path()).unwrap();

        let result = complete("test-stage".to_string(), None, true);

        std::env::set_current_dir(original_dir).unwrap();

        assert!(result.is_ok());

        let loaded_stage = load_stage("test-stage", &work_dir_path).unwrap();
        assert_eq!(loaded_stage.status, StageStatus::Completed);
    }
}
