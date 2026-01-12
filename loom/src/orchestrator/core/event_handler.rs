//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;
use chrono::Utc;
use std::io::{self, Write};
use std::path::PathBuf;

use crate::commands::status::merge_status::{check_merge_state, MergeState};
use crate::git::branch::default_branch;
use crate::models::failure::FailureInfo;
use crate::models::stage::StageStatus;
use crate::orchestrator::auto_merge::{attempt_auto_merge, is_auto_merge_enabled, AutoMergeResult};
use crate::orchestrator::monitor::MonitorEvent;
use crate::orchestrator::retry::{calculate_backoff, classify_failure, should_auto_retry};
use crate::orchestrator::signals::remove_signal;
use crate::verify::transitions::load_stage;

use super::persistence::Persistence;
use super::Orchestrator;

/// Clear the current line (status line) before printing a message.
/// This prevents output from being mangled when the status line is being updated.
fn clear_status_line() {
    // \r moves cursor to start of line, \x1B[K clears from cursor to end of line
    print!("\r\x1B[K");
    let _ = io::stdout().flush();
}

/// Trait for handling monitor events
pub(super) trait EventHandler: Persistence {
    /// Handle monitor events
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()>;

    /// Handle stage completion
    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()>;

    /// Handle session crash
    fn on_session_crashed(
        &mut self,
        session_id: &str,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    ) -> Result<()>;

    /// Handle context exhaustion (needs handoff)
    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()>;

    /// Handle merge session completion
    fn on_merge_session_completed(&mut self, session_id: &str, stage_id: &str) -> Result<()>;
}

impl EventHandler for Orchestrator {
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()> {
        for event in events {
            match event {
                MonitorEvent::StageCompleted { stage_id } => {
                    self.on_stage_completed(&stage_id)?;
                }
                MonitorEvent::StageBlocked { stage_id, reason } => {
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' blocked: {reason}");
                    self.graph.mark_blocked(&stage_id)?;
                }
                MonitorEvent::SessionContextWarning {
                    session_id,
                    usage_percent,
                } => {
                    clear_status_line();
                    eprintln!("Warning: Session '{session_id}' context at {usage_percent:.1}%");
                }
                MonitorEvent::SessionContextCritical {
                    session_id,
                    usage_percent,
                } => {
                    clear_status_line();
                    eprintln!("Critical: Session '{session_id}' context at {usage_percent:.1}%");
                }
                MonitorEvent::SessionCrashed {
                    session_id,
                    stage_id,
                    crash_report_path,
                } => {
                    self.on_session_crashed(&session_id, stage_id, crash_report_path)?;
                }
                MonitorEvent::SessionNeedsHandoff {
                    session_id,
                    stage_id,
                } => {
                    self.on_needs_handoff(&session_id, &stage_id)?;
                }
                MonitorEvent::StageWaitingForInput {
                    stage_id,
                    session_id,
                } => {
                    clear_status_line();
                    if let Some(sid) = session_id {
                        eprintln!("Stage '{stage_id}' (session '{sid}') is waiting for user input");
                    } else {
                        eprintln!("Stage '{stage_id}' is waiting for user input");
                    }
                }
                MonitorEvent::StageResumedExecution { stage_id } => {
                    clear_status_line();
                    eprintln!("Stage '{stage_id}' resumed execution after user input");
                }
                MonitorEvent::MergeSessionCompleted {
                    session_id,
                    stage_id,
                } => {
                    self.on_merge_session_completed(&session_id, &stage_id)?;
                }
                MonitorEvent::CheckpointCreated {
                    session_id,
                    task_id,
                    verification_passed,
                    warnings,
                    stage_complete,
                } => {
                    clear_status_line();
                    if !verification_passed && !warnings.is_empty() {
                        eprintln!(
                            "Checkpoint '{task_id}' (session {session_id}) created with {} warnings",
                            warnings.len()
                        );
                    } else if stage_complete {
                        eprintln!(
                            "Checkpoint '{task_id}' (session {session_id}) completed - all tasks done!"
                        );
                    } else {
                        eprintln!(
                            "Checkpoint '{task_id}' (session {session_id}) completed successfully"
                        );
                    }
                }
                MonitorEvent::SessionHung {
                    session_id,
                    stage_id,
                    stale_duration_secs,
                    last_activity,
                } => {
                    clear_status_line();
                    let stage_info = stage_id
                        .as_ref()
                        .map(|s| format!(" (stage '{s}')"))
                        .unwrap_or_default();
                    let activity_info = last_activity
                        .as_ref()
                        .map(|a| format!(", last: {a}"))
                        .unwrap_or_default();
                    eprintln!(
                        "Warning: Session '{session_id}'{stage_info} appears hung (no heartbeat for {stale_duration_secs}s{activity_info})"
                    );
                }
                MonitorEvent::HeartbeatReceived {
                    stage_id: _,
                    session_id: _,
                    context_percent: _,
                    last_tool: _,
                } => {
                    // Heartbeat events are silent - just used for internal tracking
                }
                MonitorEvent::RecoveryInitiated {
                    stage_id,
                    session_id,
                    recovery_type,
                } => {
                    clear_status_line();
                    eprintln!(
                        "Recovery initiated for stage '{stage_id}' (session '{session_id}', type: {recovery_type:?})"
                    );
                }
                MonitorEvent::StageEscalated {
                    stage_id,
                    failure_count,
                    reason,
                } => {
                    clear_status_line();
                    eprintln!(
                        "Stage '{stage_id}' escalated after {failure_count} failures: {reason}"
                    );
                }
                MonitorEvent::ContextRefreshNeeded {
                    stage_id,
                    session_id,
                    context_percent,
                } => {
                    clear_status_line();
                    eprintln!(
                        "Context refresh needed for stage '{stage_id}' (session '{session_id}', context at {context_percent:.1}%)"
                    );
                }
            }
        }
        Ok(())
    }

    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        self.graph.mark_completed(stage_id)?;

        if let Some(session) = self.active_sessions.remove(stage_id) {
            remove_signal(&session.id, &self.config.work_dir)?;
            let _ = self.backend.kill_session(&session);
        }

        self.active_worktrees.remove(stage_id);

        // Attempt auto-merge if enabled
        self.try_auto_merge(stage_id);

        Ok(())
    }

    fn on_session_crashed(
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

            // Don't override terminal states - stage may have completed before tmux died
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

            stage.status = StageStatus::Blocked;
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

    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        clear_status_line();
        eprintln!("Session '{session_id}' needs handoff for stage '{stage_id}'");

        let mut stage = self.load_stage(stage_id)?;
        stage.try_mark_needs_handoff()?;
        self.save_stage(&stage)?;

        Ok(())
    }

    fn on_merge_session_completed(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        clear_status_line();
        eprintln!("Merge session '{session_id}' completed for stage '{stage_id}'");

        // Remove the merge signal file
        remove_signal(session_id, &self.config.work_dir)?;

        // Clean up the active session
        self.active_sessions.remove(stage_id);

        // Check if the merge was successful and update stage accordingly
        let mut stage = self.load_stage(stage_id)?;

        // If stage is already marked as merged (e.g., agent ran `loom worktree remove`), we're done
        if stage.merged {
            clear_status_line();
            eprintln!("Stage '{stage_id}' merge completed successfully");
            return Ok(());
        }

        // Determine the merge point to check against
        let merge_point = self.config.base_branch.clone().unwrap_or_else(|| {
            default_branch(&self.config.repo_root).unwrap_or_else(|_| "main".to_string())
        });

        // Check if the merge was actually successful by examining git state
        match check_merge_state(&stage, &merge_point, &self.config.repo_root) {
            Ok(MergeState::Merged) => {
                // Merge succeeded - update stage
                stage.merged = true;
                stage.merge_conflict = false;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!(
                        "Warning: Failed to save stage after detecting successful merge: {e}"
                    );
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' merge verified and marked as complete");
            }
            Ok(MergeState::BranchMissing) => {
                // Branch was deleted (likely by `loom worktree remove`) - assume merge succeeded
                stage.merged = true;
                stage.merge_conflict = false;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after branch cleanup: {e}");
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' branch cleaned up, marking as merged");
            }
            Ok(MergeState::Pending) | Ok(MergeState::Conflict) | Ok(MergeState::Unknown) => {
                // Merge not complete - log next steps for the user
                eprintln!("Merge may not be complete. To finish:");
                eprintln!("  1. Verify the merge was successful: git status");
                eprintln!("  2. If merge is complete, run: loom worktree remove {stage_id}");
                eprintln!("  3. If issues remain, run: loom merge {stage_id}");
            }
            Err(e) => {
                eprintln!("Warning: Failed to verify merge state: {e}");
                eprintln!("To complete:");
                eprintln!("  1. Verify the merge was successful: git status");
                eprintln!("  2. If merge is complete, run: loom worktree remove {stage_id}");
                eprintln!("  3. If issues remain, run: loom merge {stage_id}");
            }
        }

        Ok(())
    }
}

impl Orchestrator {
    fn try_auto_merge(&self, stage_id: &str) {
        // Load the stage to check auto_merge setting
        let mut stage = match load_stage(stage_id, &self.config.work_dir) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Warning: Failed to load stage for auto-merge check: {e}");
                return;
            }
        };

        // Check if auto-merge is enabled for this stage
        // TODO: In the future, load plan_auto_merge from config file
        let plan_auto_merge = None;

        if !is_auto_merge_enabled(&stage, self.config.auto_merge, plan_auto_merge) {
            return;
        }

        // Get target branch (from config or default branch of the repo)
        let target_branch = self.config.base_branch.clone().unwrap_or_else(|| {
            default_branch(&self.config.repo_root).unwrap_or_else(|_| "main".to_string())
        });

        clear_status_line();
        eprintln!("Auto-merging stage '{stage_id}'...");

        match attempt_auto_merge(
            &stage,
            &self.config.repo_root,
            &self.config.work_dir,
            &target_branch,
            self.backend.as_ref(),
        ) {
            Ok(AutoMergeResult::Success {
                files_changed,
                insertions,
                deletions,
                ..
            }) => {
                // Mark stage as merged and save
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after merge: {e}");
                }
                clear_status_line();
                eprintln!(
                    "Stage '{stage_id}' merged: {files_changed} files, +{insertions} -{deletions}"
                );
            }
            Ok(AutoMergeResult::FastForward { .. }) => {
                // Mark stage as merged and save
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after merge: {e}");
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' merged (fast-forward)");
            }
            Ok(AutoMergeResult::AlreadyUpToDate { .. }) => {
                // Mark stage as merged and save (no changes needed, but branch is up to date)
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after merge: {e}");
                }
                clear_status_line();
                eprintln!("Stage '{stage_id}' already up to date");
            }
            Ok(AutoMergeResult::ConflictResolutionSpawned {
                session_id,
                conflicting_files,
            }) => {
                // Mark stage as having merge conflicts
                stage.merge_conflict = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage merge conflict status: {e}");
                }
                clear_status_line();
                eprintln!(
                    "Stage '{stage_id}' has {} conflict(s). Spawned resolution session: {session_id}",
                    conflicting_files.len()
                );
            }
            Ok(AutoMergeResult::NoWorktree) => {
                // Nothing to merge - stage may have been created without worktree
                // Mark as merged since there's nothing to merge
                stage.merged = true;
                if let Err(e) = self.save_stage(&stage) {
                    eprintln!("Warning: Failed to save stage after no-worktree merge: {e}");
                }
            }
            Ok(AutoMergeResult::Disabled) => {
                // Should not reach here
            }
            Err(e) => {
                clear_status_line();
                eprintln!("Auto-merge failed for '{stage_id}': {e}");
            }
        }
    }
}
