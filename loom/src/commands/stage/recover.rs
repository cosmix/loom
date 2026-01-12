//! Recovery command for stages
//!
//! Manual recovery trigger for crashed or hung stages.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::stage::StageStatus;
use crate::orchestrator::hooks::read_stage_events;
use crate::orchestrator::monitor::failure_tracking::FailureTracker;
use crate::orchestrator::signals::{
    generate_recovery_signal, LastHeartbeatInfo, RecoveryReason, RecoverySignalContent,
};
use crate::orchestrator::{heartbeat_path, read_heartbeat};
use crate::verify::transitions::{load_stage, save_stage};

/// Manually trigger recovery for a stage
///
/// This command:
/// 1. Loads the stage and validates it's in a recoverable state
/// 2. Creates a recovery signal with context from the last session
/// 3. Resets the stage to Queued status for a new session
pub fn recover(stage_id: String, force: bool) -> Result<()> {
    let work_dir = Path::new(".work");

    // Load the stage
    let mut stage = load_stage(&stage_id, work_dir)
        .with_context(|| format!("Failed to load stage: {stage_id}"))?;

    // Validate stage is in a recoverable state
    if !force {
        match stage.status {
            StageStatus::Blocked
            | StageStatus::NeedsHandoff
            | StageStatus::Executing
            | StageStatus::CompletedWithFailures
            | StageStatus::MergeBlocked => {
                // These are recoverable states
            }
            StageStatus::Completed => {
                bail!(
                    "Stage '{stage_id}' is already completed. Use --force to override."
                );
            }
            StageStatus::WaitingForDeps | StageStatus::Queued => {
                bail!(
                    "Stage '{}' is not in a failed state (status: {}). Use --force to override.",
                    stage_id,
                    stage.status
                );
            }
            StageStatus::WaitingForInput => {
                bail!(
                    "Stage '{stage_id}' is waiting for input, not crashed. Use 'loom stage resume {stage_id}' instead."
                );
            }
            StageStatus::Skipped => {
                bail!(
                    "Stage '{stage_id}' was skipped. Use 'loom stage retry {stage_id}' to re-enable."
                );
            }
            StageStatus::MergeConflict => {
                bail!(
                    "Stage '{stage_id}' has merge conflicts. Use 'loom merge {stage_id}' to resolve."
                );
            }
        }
    }

    // Get information about the previous session
    let previous_session_id = stage
        .session
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    // Try to load last heartbeat for context
    let last_heartbeat = load_last_heartbeat(work_dir, &stage_id);

    // Get recent hook events for recovery context
    let recent_events = read_stage_events(work_dir, &stage_id).unwrap_or_default();
    let last_event = recent_events
        .last()
        .map(|e| format!("{}: {}", e.event, e.timestamp));

    // Determine recovery reason based on stage state
    let recovery_reason = determine_recovery_reason(&stage);

    // Load failure tracker to get recovery attempt count
    let mut failure_tracker = FailureTracker::new();
    let _ = failure_tracker.load_from_work_dir(work_dir);
    let recovery_attempt = failure_tracker.failure_count(&stage_id) + 1;

    // Generate new session ID
    let new_session_id = generate_recovery_session_id(&stage_id);

    // Create recovery signal content
    let signal_content = match recovery_reason {
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

    // Generate the recovery signal file
    generate_recovery_signal(&signal_content, &stage, work_dir)
        .context("Failed to generate recovery signal")?;

    // Reset stage to Queued for the new session
    stage.status = StageStatus::Queued;
    stage.session = Some(new_session_id.clone());
    stage.close_reason = Some(format!(
        "Recovery initiated (attempt #{recovery_attempt})"
    ));
    stage.updated_at = chrono::Utc::now();

    save_stage(&stage, work_dir).context("Failed to save stage")?;

    println!("Recovery initiated for stage '{stage_id}'");
    println!("  Previous session: {previous_session_id}");
    println!("  New session: {new_session_id}");
    println!("  Recovery reason: {recovery_reason}");
    println!("  Attempt: #{recovery_attempt}");
    if let Some(event) = &last_event {
        println!("  Last hook event: {event}");
    }
    println!();
    println!("The stage will be picked up by the next orchestrator poll.");
    println!("Run 'loom run' to start the orchestrator if not running.");

    Ok(())
}

/// Load last heartbeat information for a stage
fn load_last_heartbeat(work_dir: &Path, stage_id: &str) -> Option<LastHeartbeatInfo> {
    let hb_path = heartbeat_path(work_dir, stage_id);
    let heartbeat = read_heartbeat(&hb_path).ok()?;

    Some(LastHeartbeatInfo {
        timestamp: heartbeat.timestamp,
        context_percent: heartbeat.context_percent,
        last_tool: heartbeat.last_tool,
        activity: heartbeat.activity,
    })
}

/// Determine recovery reason from stage state
fn determine_recovery_reason(stage: &crate::models::stage::Stage) -> RecoveryReason {
    if let Some(ref reason) = stage.close_reason {
        let reason_lower = reason.to_lowercase();
        if reason_lower.contains("crash") || reason_lower.contains("orphan") {
            return RecoveryReason::Crash;
        }
        if reason_lower.contains("hung") || reason_lower.contains("heartbeat") {
            return RecoveryReason::Hung;
        }
        if reason_lower.contains("context") || reason_lower.contains("handoff") {
            return RecoveryReason::ContextExhaustion;
        }
    }

    // Default to manual recovery
    RecoveryReason::Manual
}

/// Generate a session ID for recovery
fn generate_recovery_session_id(stage_id: &str) -> String {
    let uuid_part = uuid::Uuid::new_v4().to_string();
    let short_uuid = &uuid_part[..8];
    let timestamp = chrono::Utc::now().timestamp();
    format!("recovery-{stage_id}-{short_uuid}-{timestamp}")
}

/// Find crash report for a session
fn find_crash_report(work_dir: &Path, session_id: &str) -> Option<std::path::PathBuf> {
    let crashes_dir = work_dir.join("crashes");
    if !crashes_dir.exists() {
        return None;
    }

    // Look for crash reports matching the session ID
    std::fs::read_dir(&crashes_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            name_str.contains(session_id) && name_str.ends_with(".md")
        })
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|e| e.path())
}

/// Extract context percentage from close reason string
fn extract_context_percent(reason: &str) -> Option<f32> {
    // Look for patterns like "75%", "75.5%", "context: 75%"
    let re = regex::Regex::new(r"(\d+(?:\.\d+)?)\s*%").ok()?;
    re.captures(reason)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_context_percent() {
        assert_eq!(extract_context_percent("Context at 75%"), Some(75.0));
        assert_eq!(extract_context_percent("context: 85.5% used"), Some(85.5));
        assert_eq!(extract_context_percent("no percentage here"), None);
    }

    #[test]
    fn test_determine_recovery_reason() {
        use crate::models::stage::{Stage, StageStatus};
        use chrono::Utc;

        let mut stage = Stage {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: None,
            status: StageStatus::Blocked,
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
            auto_merge: None,
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
        };

        // No reason - should be Manual
        assert_eq!(determine_recovery_reason(&stage), RecoveryReason::Manual);

        // Crash reason
        stage.close_reason = Some("Session crashed unexpectedly".to_string());
        assert_eq!(determine_recovery_reason(&stage), RecoveryReason::Crash);

        // Hung reason
        stage.close_reason = Some("No heartbeat for 5 minutes".to_string());
        assert_eq!(determine_recovery_reason(&stage), RecoveryReason::Hung);

        // Context exhaustion
        stage.close_reason = Some("Context limit reached, handoff created".to_string());
        assert_eq!(
            determine_recovery_reason(&stage),
            RecoveryReason::ContextExhaustion
        );
    }
}
