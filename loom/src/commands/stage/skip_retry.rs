//! Skip and retry commands for stages

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::hooks::read_stage_events;
use crate::models::stage::StageStatus;
use crate::orchestrator::monitor::failure_tracking::FailureTracker;
use crate::orchestrator::signals::{
    generate_recovery_signal, RecoveryReason, RecoverySignalContent,
};
use crate::orchestrator::skip::skip_stage;
use crate::verify::transitions::{load_stage, save_stage};

use super::recover::{
    determine_recovery_reason, extract_context_percent, find_crash_report,
    generate_recovery_session_id, load_last_heartbeat,
};

/// Skip a stage
pub fn skip(stage_id: String, reason: Option<String>) -> Result<()> {
    let work_dir = Path::new(".work");

    skip_stage(&stage_id, reason.clone(), work_dir)?;

    println!("Stage '{stage_id}' skipped.");
    if let Some(r) = reason {
        println!("Reason: {r}");
    }
    println!("Note: Dependent stages will remain blocked.");

    Ok(())
}

/// Retry a failed, crashed, or hung stage
///
/// Generates a recovery signal with context when the stage was crashed or
/// hung, or when --context is provided. Replaces the old `recover` command.
pub fn retry(stage_id: String, force: bool, context: Option<String>) -> Result<()> {
    let work_dir = Path::new(".work");

    let mut stage = load_stage(&stage_id, work_dir)?;

    // Defense-in-depth: check for active session to prevent parallel session spawning
    if let Some(ref session_id) = stage.session {
        let session_path = work_dir.join("sessions").join(format!("{session_id}.md"));
        if session_path.exists() {
            eprintln!(
                "WARNING: Stage '{}' may have an active session ({})",
                stage_id, session_id
            );
            eprintln!("  If the session is still running, retry will create a parallel session.");
            eprintln!(
                "  Fix issues in the current session and run 'loom stage complete {stage_id}'."
            );
            if !force {
                bail!("Stage has active session. Use --force to override.");
            }
            eprintln!("  --force used, proceeding with retry despite active session.");
        }
    }

    // Allow retry for Blocked, CompletedWithFailures, MergeBlocked, NeedsHandoff,
    // and Executing states (the latter two were previously only in `recover`)
    let retryable = matches!(
        stage.status,
        StageStatus::Blocked
            | StageStatus::CompletedWithFailures
            | StageStatus::MergeBlocked
            | StageStatus::NeedsHandoff
            | StageStatus::Executing
    );

    if !retryable {
        bail!(
            "Cannot retry stage in status: {}. \
             Only blocked, completed-with-failures, merge-blocked, \
             needs-handoff, or executing stages can be retried.",
            stage.status
        );
    }

    let max = stage.max_retries.unwrap_or(3);
    if !force && stage.retry_count >= max {
        bail!(
            "Stage '{}' has exceeded retry limit ({}/{}). Use --force to override.",
            stage_id,
            stage.retry_count,
            max
        );
    }

    // Determine whether we need a recovery signal.
    // Generate one if explicit --context is provided, or if the close_reason
    // suggests a crash/hung/context-exhaustion scenario.
    let recovery_reason = if context.is_some() {
        Some(RecoveryReason::Manual)
    } else {
        let auto_reason = determine_recovery_reason(&stage);
        // Only auto-generate recovery signal for non-manual reasons
        if auto_reason != RecoveryReason::Manual {
            Some(auto_reason)
        } else {
            None
        }
    };

    // Get previous session info before we overwrite it
    let previous_session_id = stage
        .session
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    // Reset or increment for retry
    if force {
        stage.retry_count = 0;
        stage.failure_info = None;
    } else {
        stage.retry_count += 1;
    }
    stage.last_failure_at = None;
    stage.try_mark_queued()?;

    // Generate recovery signal if needed
    if let Some(reason) = recovery_reason {
        let last_heartbeat = load_last_heartbeat(work_dir, &stage_id);

        let recent_events = read_stage_events(work_dir, &stage_id).unwrap_or_default();
        let last_event = recent_events
            .last()
            .map(|e| format!("{}: {}", e.event, e.timestamp));

        let mut failure_tracker = FailureTracker::new();
        let _ = failure_tracker.load_from_work_dir(work_dir);
        let recovery_attempt = failure_tracker.failure_count(&stage_id) + 1;

        let new_session_id = generate_recovery_session_id(&stage_id);

        let signal_content = match reason {
            RecoveryReason::Crash => RecoverySignalContent::for_crash(
                new_session_id.clone(),
                stage_id.clone(),
                previous_session_id.clone(),
                find_crash_report(work_dir, &previous_session_id),
                recovery_attempt,
            ),
            RecoveryReason::Hung => RecoverySignalContent::for_hung(
                new_session_id.clone(),
                stage_id.clone(),
                previous_session_id.clone(),
                last_heartbeat,
                recovery_attempt,
            ),
            RecoveryReason::ContextExhaustion => {
                let context_pct = stage
                    .close_reason
                    .as_ref()
                    .and_then(|r| extract_context_percent(r))
                    .unwrap_or(75.0);
                RecoverySignalContent::for_context_exhaustion(
                    new_session_id.clone(),
                    stage_id.clone(),
                    previous_session_id.clone(),
                    context_pct,
                    recovery_attempt,
                )
            }
            RecoveryReason::Manual => RecoverySignalContent::for_manual(
                new_session_id.clone(),
                stage_id.clone(),
                previous_session_id.clone(),
                recovery_attempt,
            ),
        };

        generate_recovery_signal(&signal_content, &stage, work_dir)
            .context("Failed to generate recovery signal")?;

        stage.session = Some(new_session_id.clone());
        stage.close_reason = if let Some(ref ctx_msg) = context {
            Some(format!(
                "Recovery initiated (attempt #{recovery_attempt}): {ctx_msg}"
            ))
        } else {
            Some(format!("Recovery initiated (attempt #{recovery_attempt})"))
        };
        stage.updated_at = chrono::Utc::now();

        save_stage(&stage, work_dir)?;

        println!("Recovery initiated for stage '{stage_id}'");
        println!("  Previous session: {previous_session_id}");
        println!("  New session: {new_session_id}");
        println!("  Recovery reason: {reason}");
        println!("  Attempt: #{recovery_attempt}");
        if let Some(ref ctx_msg) = context {
            println!("  Context: {ctx_msg}");
        }
        if let Some(event) = &last_event {
            println!("  Last hook event: {event}");
        }
        println!();
        println!("The stage will be picked up by the next orchestrator poll.");
        println!("Run 'loom run' to start the orchestrator if not running.");
    } else {
        // Simple retry without recovery signal
        save_stage(&stage, work_dir)?;

        println!("Stage '{stage_id}' queued for retry.");
        if force {
            println!("Retry count reset (--force used).");
        }
    }

    Ok(())
}
