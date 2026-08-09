//! Recovery context helpers shared by the retry command.

use std::path::Path;

use crate::orchestrator::signals::{LastHeartbeatInfo, RecoveryReason};
use crate::orchestrator::{heartbeat_path, read_heartbeat};

/// Load last heartbeat information for a stage
pub(crate) fn load_last_heartbeat(work_dir: &Path, stage_id: &str) -> Option<LastHeartbeatInfo> {
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
pub(crate) fn determine_recovery_reason(stage: &crate::models::stage::Stage) -> RecoveryReason {
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
pub(crate) fn generate_recovery_session_id(stage_id: &str) -> String {
    let uuid_part = uuid::Uuid::new_v4().to_string();
    let short_uuid = &uuid_part[..8];
    let timestamp = chrono::Utc::now().timestamp();
    format!("recovery-{stage_id}-{short_uuid}-{timestamp}")
}

/// Find crash report for a session
pub(crate) fn find_crash_report(work_dir: &Path, session_id: &str) -> Option<std::path::PathBuf> {
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
pub(crate) fn extract_context_percent(reason: &str) -> Option<f32> {
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
        use crate::models::stage::{Implementers, Stage, StageStatus, StageType};
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
            started_at: None,
            duration_secs: None,
            execution_secs: None,
            attempt_started_at: None,
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
            verification_status: Default::default(),
            context_budget: None,
            artifacts: Vec::new(),
            wiring: Vec::new(),
            wiring_tests: Vec::new(),
            dead_code_check: None,
            before_stage: Vec::new(),
            after_stage: Vec::new(),
            fix_attempts: 0,
            dispute_count: 0,
            evidence_rounds: 0,
            amendments_applied: 0,
            sandbox: Default::default(),
            execution_mode: None,
            max_fix_attempts: None,
            review_reason: None,
            bug_fix: None,
            regression_test: None,
            model: None,
            reasoning_effort: None,
            code_review: None,
            ultracode: false,
            implementers: Implementers::default(),
            subagent_timeout_secs: None,
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
