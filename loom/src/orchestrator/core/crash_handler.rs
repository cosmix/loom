//! Session crash handling and retry logic

use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;

use crate::models::failure::FailureInfo;
use crate::models::stage::StageStatus;
use crate::orchestrator::retry::{calculate_backoff, classify_failure, should_auto_retry};

use super::persistence::Persistence;
use super::{clear_status_line, Orchestrator};

impl Orchestrator {
    pub(super) fn handle_session_crashed(
        &mut self,
        session_id: &str,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    ) -> Result<()> {
        // Check if we've already reported this crash to avoid duplicate messages
        if self.reported_crashes.contains(session_id) {
            return Ok(());
        }
        self.reported_crashes.insert(session_id.to_string());

        if let Some(sid) = stage_id {
            self.active_sessions.remove(&sid);

            let mut stage = self.load_stage(&sid)?;

            // Don't override terminal states - stage may have completed before session died
            if matches!(stage.status, StageStatus::Completed) {
                // Stage already completed successfully, just clean up
                return Ok(());
            }

            clear_status_line();
            eprintln!("Session '{session_id}' crashed for stage '{sid}'");

            // Build the failure reason
            let reason = crash_report_path
                .as_ref()
                .map(|p| format!("Session crashed - see crash report at {}", p.display()))
                .unwrap_or_else(|| "Session crashed".to_string());

            // Classify the failure
            let failure_type = classify_failure(&reason);

            // Update failure information
            stage.failure_info = Some(FailureInfo {
                failure_type: failure_type.clone(),
                detected_at: Utc::now(),
                evidence: vec![reason.clone()],
            });
            stage.last_failure_at = Some(Utc::now());
            stage.retry_count += 1;
            stage.close_reason = Some(reason);

            // Check if auto-retry is eligible (default max_retries = 3)
            let max = stage.max_retries.unwrap_or(3);
            if should_auto_retry(&failure_type, stage.retry_count, max) {
                let backoff = calculate_backoff(stage.retry_count, 30, 300);
                clear_status_line();
                eprintln!(
                    "Stage '{}' crashed (attempt {}/{}). Will retry in {}s...",
                    sid,
                    stage.retry_count,
                    max,
                    backoff.as_secs()
                );
            } else if stage.retry_count >= max {
                clear_status_line();
                eprintln!(
                    "Stage '{}' failed after {} attempts. Run `loom diagnose {}` for help.",
                    sid, stage.retry_count, sid
                );
            }

            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
            }

            // Transition to Blocked status with validation
            if let Err(e) = stage.try_mark_blocked() {
                eprintln!("Warning: Failed to transition stage to Blocked: {e}");
                eprintln!("Current status: {:?}", stage.status);
            }
            self.save_stage(&stage)?;

            self.graph.mark_blocked(&sid)?;
        } else {
            clear_status_line();
            eprintln!("Session '{session_id}' crashed (no stage association)");
            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
            }
        }

        Ok(())
    }
}
