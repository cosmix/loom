//! Diagnose command - spawns Claude Code session to analyze failed stages

use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

use crate::models::stage::StageStatus;
use crate::verify::transitions::load_stage;

/// Execute the diagnose command
pub fn execute(stage_id: &str) -> Result<()> {
    let work_dir = PathBuf::from(".work");

    // Load and validate stage
    let stage = load_stage(stage_id, &work_dir)?;

    if stage.status != StageStatus::Blocked {
        bail!(
            "Cannot diagnose stage in status: {}. Only blocked stages can be diagnosed.",
            stage.status
        );
    }

    // Gather diagnostic context
    let crash_report = load_crash_report(stage_id, &work_dir);
    let git_status = get_worktree_git_status(&stage, &work_dir);
    let git_diff = get_worktree_git_diff(&stage, &work_dir);

    // Generate session ID and signal
    let session_id = format!(
        "diag-{}",
        Uuid::new_v4()
            .to_string()
            .split('-')
            .next()
            .unwrap_or("unknown")
    );

    let signal_path = generate_diagnosis_signal(
        &session_id,
        &stage,
        crash_report.as_deref(),
        git_status.as_deref(),
        git_diff.as_deref(),
        &work_dir,
    )?;

    println!("Diagnosis signal generated: {}", signal_path.display());

    // Determine working directory for the session
    let session_cwd = stage
        .worktree
        .as_ref()
        .and_then(|wt| {
            let path = work_dir.parent()?.join(".worktrees").join(wt);
            if path.exists() {
                Some(path)
            } else {
                None
            }
        })
        .unwrap_or_else(|| work_dir.parent().unwrap_or(&work_dir).to_path_buf());

    println!("Spawning diagnosis session...");
    println!("Working directory: {}", session_cwd.display());

    // Try to spawn in tmux if available
    let tmux_result = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &session_id,
            "-c",
            &session_cwd.to_string_lossy(),
            "claude",
        ])
        .status();

    match tmux_result {
        Ok(status) if status.success() => {
            println!("Diagnosis session '{session_id}' started in tmux.");
            println!();
            println!("To attach: tmux attach -t {session_id}");
            println!("Signal file: {}", signal_path.display());
        }
        _ => {
            // Fall back: instruct user to run claude manually
            println!();
            println!("Could not spawn tmux session. Please run manually:");
            println!("  cd {}", session_cwd.display());
            println!("  claude");
            println!();
            println!("The signal file is at: {}", signal_path.display());
        }
    }

    Ok(())
}

/// Load crash report for a stage if it exists
fn load_crash_report(stage_id: &str, work_dir: &Path) -> Option<String> {
    let crashes_dir = work_dir.join("crashes");
    if !crashes_dir.exists() {
        return None;
    }

    // Look for crash reports matching this stage
    let entries = fs::read_dir(&crashes_dir).ok()?;

    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.contains(stage_id) {
                if let Ok(content) = fs::read_to_string(&path) {
                    return Some(content);
                }
            }
        }
    }

    None
}

/// Get git status from stage's worktree if it exists
fn get_worktree_git_status(
    stage: &crate::models::stage::Stage,
    work_dir: &Path,
) -> Option<String> {
    let wt = stage.worktree.as_ref()?;
    let worktree_path = work_dir.parent()?.join(".worktrees").join(wt);

    if !worktree_path.exists() {
        return None;
    }

    Command::new("git")
        .args(["status", "--short"])
        .current_dir(&worktree_path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

/// Get git diff from stage's worktree if it exists
fn get_worktree_git_diff(
    stage: &crate::models::stage::Stage,
    work_dir: &Path,
) -> Option<String> {
    let wt = stage.worktree.as_ref()?;
    let worktree_path = work_dir.parent()?.join(".worktrees").join(wt);

    if !worktree_path.exists() {
        return None;
    }

    Command::new("git")
        .args(["diff", "HEAD~1..HEAD", "--stat"])
        .current_dir(&worktree_path)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

/// Generate a diagnosis signal file for analyzing a failed stage
fn generate_diagnosis_signal(
    session_id: &str,
    stage: &crate::models::stage::Stage,
    crash_report: Option<&str>,
    git_status: Option<&str>,
    git_diff: Option<&str>,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");

    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    let signal_path = signals_dir.join(format!("{session_id}.md"));
    let content = format_diagnosis_signal(session_id, stage, crash_report, git_status, git_diff);

    fs::write(&signal_path, content)
        .with_context(|| format!("Failed to write signal file: {}", signal_path.display()))?;

    Ok(signal_path)
}

/// Format the diagnosis signal content
fn format_diagnosis_signal(
    session_id: &str,
    stage: &crate::models::stage::Stage,
    crash_report: Option<&str>,
    git_status: Option<&str>,
    git_diff: Option<&str>,
) -> String {
    let mut content = String::new();

    content.push_str(&format!("# Diagnosis Signal: {session_id}\n\n"));

    // Context
    content.push_str("## Diagnosis Context\n\n");
    content.push_str("You are analyzing a **blocked stage** to diagnose why it failed.\n\n");
    content.push_str("- Review the stage details, failure information, and evidence below\n");
    content.push_str("- Investigate the root cause of the failure\n");
    content.push_str("- Provide actionable recommendations for fixing the issue\n\n");

    // Execution rules
    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` rules. Key reminders:\n");
    content.push_str("- **Analyze thoroughly** - don't just fix symptoms\n");
    content.push_str("- **Use TodoWrite** to track your investigation\n");
    content.push_str("- **Document findings** in a handoff file if needed\n");
    content
        .push_str("- **Do NOT implement fixes** - diagnose only, unless explicitly requested\n\n");

    // Target information
    content.push_str("## Target\n\n");
    content.push_str(&format!("- **Session**: {session_id}\n"));
    content.push_str(&format!("- **Stage**: {}\n", stage.id));
    content.push_str(&format!("- **Status**: {}\n", stage.status));
    if let Some(reason) = &stage.close_reason {
        content.push_str(&format!("- **Block Reason**: {reason}\n"));
    }
    content.push('\n');

    // Stage context
    content.push_str("## Stage Context\n\n");
    content.push_str(&format!("**{}**\n\n", stage.name));
    if let Some(desc) = &stage.description {
        content.push_str(&format!("{desc}\n\n"));
    }

    // Acceptance criteria
    if !stage.acceptance.is_empty() {
        content.push_str("### Acceptance Criteria\n\n");
        for criterion in &stage.acceptance {
            content.push_str(&format!("- [ ] {criterion}\n"));
        }
        content.push('\n');
    }

    // Failure information
    if let Some(failure_info) = &stage.failure_info {
        content.push_str("## Failure Information\n\n");
        content.push_str(&format!("- **Type**: {:?}\n", failure_info.failure_type));
        content.push_str(&format!(
            "- **Detected At**: {}\n",
            failure_info.detected_at
        ));

        if !failure_info.evidence.is_empty() {
            content.push_str("\n### Evidence\n\n");
            for evidence in &failure_info.evidence {
                content.push_str(&format!("- {evidence}\n"));
            }
            content.push('\n');
        }
    }

    // Crash report
    if let Some(report) = crash_report {
        content.push_str("## Crash Report\n\n");
        content.push_str("```\n");
        content.push_str(report);
        content.push_str("\n```\n\n");
    }

    // Git status
    if let Some(status) = git_status {
        content.push_str("## Git Status\n\n");
        content.push_str("```\n");
        content.push_str(status);
        content.push_str("\n```\n\n");
    }

    // Git diff
    if let Some(diff) = git_diff {
        content.push_str("## Git Diff\n\n");
        content.push_str("```\n");
        content.push_str(diff);
        content.push_str("\n```\n\n");
    }

    // Investigation tasks
    content.push_str("## Your Investigation Tasks\n\n");
    content.push_str("1. Review all failure information and evidence above\n");
    content.push_str("2. Examine relevant code files and logs\n");
    content.push_str("3. Identify the root cause of the failure\n");
    content.push_str("4. Document your findings\n");
    content.push_str("5. Provide actionable recommendations for fixing the issue\n\n");

    // Retry information
    if stage.retry_count > 0 {
        content.push_str("## Retry History\n\n");
        content.push_str(&format!("- **Retry Count**: {}\n", stage.retry_count));
        if let Some(max) = stage.max_retries {
            content.push_str(&format!("- **Max Retries**: {max}\n"));
        }
        if let Some(last_failure) = stage.last_failure_at {
            content.push_str(&format!("- **Last Failure**: {last_failure}\n"));
        }
        content.push('\n');
    }

    content.push_str("## Expected Deliverables\n\n");
    content.push_str("- Root cause analysis\n");
    content.push_str("- Recommended fix or workaround\n");
    content.push_str(
        "- Whether the stage should be retried, reset, or requires manual intervention\n",
    );

    content
}
