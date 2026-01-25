//! Recovery signal generation for crashed/hung sessions.
//!
//! When a session crashes or hangs, the orchestrator generates a recovery signal
//! that contains context about what was happening and how to continue.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::models::stage::Stage;

use super::generate::build_embedded_context_with_stage;
use super::recovery_format::format_recovery_signal;
use super::recovery_types::RecoverySignalContent;

/// Generate a recovery signal file
pub fn generate_recovery_signal(
    content: &RecoverySignalContent,
    stage: &Stage,
    work_dir: &Path,
) -> Result<PathBuf> {
    let signals_dir = work_dir.join("signals");
    if !signals_dir.exists() {
        fs::create_dir_all(&signals_dir).context("Failed to create signals directory")?;
    }

    // Build embedded context including any available handoff
    let handoff_file = find_latest_handoff_for_stage(work_dir, &content.stage_id);
    let embedded_context = build_embedded_context_with_stage(
        work_dir,
        handoff_file.as_deref(),
        Some(&content.stage_id),
    );

    let signal_path = signals_dir.join(format!("{}.md", &content.session_id));
    let signal_content = format_recovery_signal(content, stage, &embedded_context);

    fs::write(&signal_path, &signal_content)
        .with_context(|| format!("Failed to write recovery signal: {}", signal_path.display()))?;

    Ok(signal_path)
}

/// Find the latest handoff file for a stage
pub fn find_latest_handoff_for_stage(work_dir: &Path, stage_id: &str) -> Option<String> {
    let handoffs_dir = work_dir.join("handoffs");
    if !handoffs_dir.exists() {
        return None;
    }

    let mut handoffs: Vec<_> = fs::read_dir(&handoffs_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name_str = name.to_string_lossy();
            name_str.ends_with(".md") && name_str.contains(stage_id)
        })
        .collect();

    // Sort by modification time, newest first
    handoffs.sort_by_key(|e| std::cmp::Reverse(e.metadata().ok().and_then(|m| m.modified().ok())));

    handoffs.first().map(|e| {
        e.path()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::signals::recovery_types::{LastHeartbeatInfo, RecoveryReason};
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_stage() -> Stage {
        Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            description: Some("Test description".to_string()),
            status: crate::models::stage::StageStatus::Executing,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec!["cargo test".to_string()],
            setup: vec![],
            files: vec!["src/lib.rs".to_string()],
            stage_type: crate::models::stage::StageType::default(),
            plan_id: Some("test-plan".to_string()),
            worktree: Some(".worktrees/test-stage".to_string()),
            session: Some("session-123".to_string()),
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            started_at: None,
            duration_secs: None,
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
        }
    }

    #[test]
    fn test_recovery_signal_for_crash() {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let content = RecoverySignalContent::for_crash(
            "session-new".to_string(),
            "test-stage".to_string(),
            "session-old".to_string(),
            Some(PathBuf::from(".work/crashes/crash-123.md")),
            1,
        );

        assert_eq!(content.reason, RecoveryReason::Crash);
        assert_eq!(content.session_id, "session-new");
        assert_eq!(content.previous_session_id, "session-old");
        assert_eq!(content.recovery_attempt, 1);
        assert!(content.crash_report_path.is_some());
    }

    #[test]
    fn test_recovery_signal_for_hung() {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let hb = LastHeartbeatInfo {
            timestamp: Utc::now(),
            context_percent: Some(45.0),
            last_tool: Some("Bash".to_string()),
            activity: Some("Running tests".to_string()),
        };

        let content = RecoverySignalContent::for_hung(
            "session-new".to_string(),
            "test-stage".to_string(),
            "session-old".to_string(),
            Some(hb),
            2,
        );

        assert_eq!(content.reason, RecoveryReason::Hung);
        assert!(content.last_heartbeat.is_some());
        assert_eq!(content.recovery_attempt, 2);
    }

    #[test]
    fn test_generate_recovery_signal() -> Result<()> {
        use crate::orchestrator::signals::recovery_types::RecoverySignalContent;
        let tmp = TempDir::new()?;
        let work_dir = tmp.path();

        // Create signals directory
        fs::create_dir_all(work_dir.join("signals"))?;

        let stage = create_test_stage();
        let content = RecoverySignalContent::for_crash(
            "session-recovery".to_string(),
            "test-stage".to_string(),
            "session-crashed".to_string(),
            None,
            1,
        );

        let path = generate_recovery_signal(&content, &stage, work_dir)?;
        assert!(path.exists());

        let signal_content = fs::read_to_string(&path)?;
        assert!(signal_content.contains("## Recovery Context"));
        assert!(signal_content.contains("Session crashed"));
        assert!(signal_content.contains("session-crashed"));

        Ok(())
    }
}
