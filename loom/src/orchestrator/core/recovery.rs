//! Error recovery and state synchronization

use anyhow::Result;
use std::io::{self, Write};

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::retry::{calculate_backoff, is_backoff_elapsed, should_auto_retry};
use crate::parser::frontmatter::parse_from_markdown;

use super::clear_status_line;
use super::persistence::Persistence;
use super::Orchestrator;

/// Trait for recovery operations
pub(super) trait Recovery: Persistence {
    /// Sync the execution graph with existing stage file statuses.
    /// This syncs FROM files TO graph.
    fn sync_graph_with_stage_files(&mut self) -> Result<()>;

    /// Sync queued status from graph back to stage files.
    /// This ensures files reflect when dependencies are satisfied.
    /// This syncs FROM graph TO files.
    fn sync_queued_status_to_files(&mut self) -> Result<()>;

    /// Recover orphaned sessions (process died but session/stage files exist).
    fn recover_orphaned_sessions(&mut self) -> Result<usize>;

    /// Check if all stages are in a terminal state (for watch mode exit)
    fn all_stages_terminal(&self) -> bool;

    /// Print a status update showing current stage counts
    fn print_status_update(&self);
}

/// Check if a blocked stage is eligible for automatic retry.
///
/// A stage is eligible for retry if:
/// - It has failure_info with a retryable failure_type (SessionCrash or Timeout)
/// - retry_count < max_retries (default 3)
/// - Sufficient time has elapsed since the last failure (exponential backoff)
///
/// # Arguments
/// * `stage` - The stage to check
///
/// # Returns
/// `true` if the stage should be automatically retried, `false` otherwise
fn check_retry_eligibility(stage: &Stage) -> bool {
    let Some(ref info) = stage.failure_info else {
        return false;
    };

    let max = stage.max_retries.unwrap_or(3);
    if !should_auto_retry(&info.failure_type, stage.retry_count, max) {
        return false;
    }

    // Calculate backoff: base 30s, max 300s (5 minutes)
    let backoff = calculate_backoff(stage.retry_count, 30, 300);
    is_backoff_elapsed(stage.last_failure_at, backoff)
}

impl Recovery for Orchestrator {
    fn sync_graph_with_stage_files(&mut self) -> Result<()> {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return Ok(());
        }

        // Read all stage files and sync their status to the graph
        for entry in std::fs::read_dir(&stages_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Extract stage ID from filename (handles prefixed format like 01-stage-id.md)
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let stage_id = if let Some(rest) = filename.strip_prefix(|c: char| c.is_ascii_digit()) {
                // Remove leading digits and dash
                rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
            } else {
                filename
            };

            if stage_id.is_empty() {
                continue;
            }

            // Load the stage and sync status
            // NOTE: We use stage.id (from YAML frontmatter) for graph operations,
            // not stage_id (from filename), because the graph is built using frontmatter IDs.
            if let Ok(mut stage) = self.load_stage(stage_id) {
                tracing::debug!(
                    stage_id = %stage.id,
                    status = ?stage.status,
                    merged = stage.merged,
                    "[sync_graph_with_stage_files] Loaded stage"
                );
                // Always sync outputs to the graph so they're available for dependent stages
                if !stage.outputs.is_empty() {
                    self.graph
                        .set_node_outputs(&stage.id, stage.outputs.clone());
                }

                match stage.status {
                    StageStatus::Completed => {
                        // IMPORTANT: Set merged status FIRST, before mark_completed().
                        // mark_completed() triggers update_ready_status() which needs the
                        // correct merged value to determine if dependent stages are ready.
                        tracing::debug!(
                            stage_id = %stage.id,
                            merged = stage.merged,
                            "[sync_graph_with_stage_files] Completed stage"
                        );
                        self.graph.set_node_merged(&stage.id, stage.merged);
                        // Now mark as completed - this triggers update_ready_status() which
                        // will see the correct merged value set above
                        if let Err(e) = self.graph.mark_completed(&stage.id) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Queued => {
                        // Sync Ready status from stage files to graph
                        // This handles stages marked Ready by `loom verify` -> trigger_dependents()
                        if let Err(e) = self.graph.mark_queued(&stage.id) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Executing => {
                        // Mark as executing in graph to track active sessions
                        if let Err(e) = self.graph.mark_executing(&stage.id) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Blocked => {
                        // Check if the blocked stage is eligible for automatic retry
                        if check_retry_eligibility(&stage) {
                            // Re-queue the stage for retry
                            if stage.try_mark_queued().is_ok() {
                                clear_status_line();
                                tracing::warn!(
                                    stage_id = %stage.id,
                                    attempt = stage.retry_count + 1,
                                    "Auto-retrying stage"
                                );

                                // ATOMIC UPDATE PATTERN:
                                // 1. Save original graph state for potential rollback
                                // 2. Update graph first (tentatively)
                                // 3. Save file
                                // 4. If save fails, rollback graph to original state
                                let original_graph_status =
                                    self.graph.get_node(&stage.id).map(|n| n.status.clone());

                                // Update graph first
                                if let Err(e) = self.graph.mark_queued(&stage.id) {
                                    tracing::warn!(
                                        "Failed to sync graph status for stage {}: {}",
                                        stage.id,
                                        e
                                    );
                                }

                                // Now save the file
                                if let Err(e) = self.save_stage(&stage) {
                                    tracing::warn!(error = %e, "Failed to save stage during retry");
                                    // Rollback graph to original state
                                    if let Some(StageStatus::Blocked) = original_graph_status {
                                        let _ =
                                            self.graph.mark_status(&stage.id, StageStatus::Blocked);
                                    }
                                }
                            }
                        } else {
                            // Not eligible for retry, just mark as blocked in graph
                            if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::Blocked)
                            {
                                tracing::warn!(
                                    "Failed to sync graph status for stage {}: {}",
                                    stage.id,
                                    e
                                );
                            }
                        }
                    }
                    StageStatus::WaitingForInput => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::WaitingForInput)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::NeedsHandoff => {
                        if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::NeedsHandoff)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::MergeConflict => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::MergeConflict)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::CompletedWithFailures => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::CompletedWithFailures)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::MergeBlocked => {
                        if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::MergeBlocked)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::Skipped => {
                        if let Err(e) = self.graph.mark_status(&stage.id, StageStatus::Skipped) {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::WaitingForDeps => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::WaitingForDeps)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                    StageStatus::NeedsHumanReview => {
                        if let Err(e) = self
                            .graph
                            .mark_status(&stage.id, StageStatus::NeedsHumanReview)
                        {
                            tracing::warn!(
                                "Failed to sync graph status for stage {}: {}",
                                stage.id,
                                e
                            );
                        }
                    }
                }
            }
        }

        // After syncing all stage statuses, refresh ready status to ensure
        // dependent stages get marked as Queued when their dependencies complete.
        // This handles cases where stages are processed out of topological order.
        self.graph.refresh_ready_status();

        Ok(())
    }

    fn sync_queued_status_to_files(&mut self) -> Result<()> {
        // Get all nodes that are Queued in the graph
        let queued_stage_ids: Vec<String> = self
            .graph
            .all_nodes()
            .iter()
            .filter(|node| node.status == StageStatus::Queued)
            .map(|node| node.id.clone())
            .collect();

        // For each queued stage, update the file if it's still WaitingForDeps
        for stage_id in queued_stage_ids {
            if let Ok(mut stage) = self.load_stage(&stage_id) {
                // Only update if the file says WaitingForDeps but graph says Queued
                if stage.status == StageStatus::WaitingForDeps {
                    // Use validated transition
                    if stage.try_mark_queued().is_ok() {
                        self.save_stage(&stage)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn recover_orphaned_sessions(&mut self) -> Result<usize> {
        let sessions_dir = self.config.work_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(0);
        }

        let mut recovered = 0;

        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Load session from file
            let content = std::fs::read_to_string(&path)?;
            let session: Session = match parse_from_markdown(&content, "Session") {
                Ok(s) => s,
                Err(_) => continue,
            };

            // Check if session is still running
            let is_running = self.backend.is_session_alive(&session).unwrap_or(false);

            if !is_running {
                // Orphaned session - get stage ID and reset it
                if let Some(stage_id) = &session.stage_id {
                    // Load the stage
                    if let Ok(mut stage) = self.load_stage(stage_id) {
                        // Recover if stage was Executing, NeedsHandoff, or Blocked due to crash
                        if matches!(
                            stage.status,
                            StageStatus::Executing
                                | StageStatus::NeedsHandoff
                                | StageStatus::Blocked
                        ) {
                            clear_status_line();
                            tracing::warn!(
                                stage_id = %stage_id,
                                status = ?stage.status,
                                "Recovering orphaned stage"
                            );

                            // Reset stage to Ready using validated transition
                            // NeedsHandoff -> Queued and Blocked -> Queued are valid transitions
                            // Executing -> Queued is not valid, so we go through Blocked first
                            if stage.status == StageStatus::Executing {
                                // Executing -> Blocked (intermediate step for recovery)
                                if let Err(e) = stage.try_mark_blocked() {
                                    tracing::warn!(error = %e, "Failed to transition Executing -> Blocked during recovery");
                                }
                            }
                            // Now Blocked/NeedsHandoff -> Queued is valid
                            if let Err(e) = stage.try_mark_queued() {
                                // Log a warning that validation was bypassed for recovery
                                tracing::warn!(
                                    error = %e,
                                    status = ?stage.status,
                                    "State transition validation failed during orphaned session recovery, bypassing"
                                );
                                stage.status = StageStatus::Queued;
                            }
                            stage.session = None;
                            stage.close_reason = Some("Session crashed/orphaned".to_string());
                            stage.updated_at = chrono::Utc::now();

                            // ATOMIC UPDATE PATTERN:
                            // 1. Save original graph state for potential rollback
                            // 2. Update graph first (tentatively)
                            // 3. Save file
                            // 4. If save fails, rollback graph to original state
                            let original_graph_status =
                                self.graph.get_node(stage_id).map(|n| n.status.clone());

                            // Update graph first - only if not in terminal state
                            let graph_updated = if let Some(node) = self.graph.get_node(stage_id) {
                                if node.status != StageStatus::Completed {
                                    if let Err(e) = self.graph.mark_queued(stage_id) {
                                        tracing::warn!(
                                            "Failed to sync graph status for stage {}: {}",
                                            stage_id,
                                            e
                                        );
                                    }
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            // Now save the file
                            if let Err(e) = self.save_stage(&stage) {
                                // Rollback graph to original state if we updated it
                                if graph_updated {
                                    if let Some(original_status) = original_graph_status {
                                        let _ = self.graph.mark_status(stage_id, original_status);
                                    }
                                }
                                return Err(e);
                            }

                            recovered += 1;
                        }
                    }
                }

                // Remove the orphaned session file
                let _ = std::fs::remove_file(&path);

                // Remove the orphaned signal file
                let signal_path = self
                    .config
                    .work_dir
                    .join("signals")
                    .join(format!("{}.md", session.id));
                let _ = std::fs::remove_file(&signal_path);
            }
        }

        Ok(recovered)
    }

    fn all_stages_terminal(&self) -> bool {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return true;
        }

        for node in self.graph.all_nodes() {
            // Check graph status first
            match node.status {
                StageStatus::Completed => continue,
                StageStatus::Blocked => continue,
                StageStatus::Skipped => continue,
                StageStatus::MergeConflict => continue, // Terminal until resolved
                StageStatus::CompletedWithFailures => continue, // Terminal until retried
                StageStatus::MergeBlocked => continue,  // Terminal until fixed
                StageStatus::NeedsHumanReview => continue, // Waiting for human review
                StageStatus::WaitingForDeps
                | StageStatus::Queued
                | StageStatus::Executing
                | StageStatus::WaitingForInput
                | StageStatus::NeedsHandoff => {
                    // Need to check the actual stage file for held status
                    if let Ok(stage) = self.load_stage(&node.id) {
                        match stage.status {
                            StageStatus::Blocked | StageStatus::Completed => {
                                continue;
                            }
                            StageStatus::Queued | StageStatus::WaitingForDeps if stage.held => {
                                continue
                            }
                            _ => return false,
                        }
                    } else {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn print_status_update(&self) {
        let nodes = self.graph.all_nodes();
        let mut running = 0;
        let mut pending = 0;
        let mut completed = 0;
        let mut blocked = 0;

        for node in nodes {
            match node.status {
                StageStatus::Executing => running += 1,
                StageStatus::WaitingForDeps | StageStatus::Queued => pending += 1,
                StageStatus::Completed => completed += 1,
                StageStatus::Blocked => blocked += 1,
                StageStatus::Skipped => completed += 1, // Count skipped as completed for status display
                StageStatus::WaitingForInput => running += 1, // Paused but still active
                StageStatus::NeedsHandoff => running += 1, // Needs continuation but still in progress
                StageStatus::MergeConflict => blocked += 1, // Blocked on conflict resolution
                StageStatus::CompletedWithFailures => blocked += 1, // Failed acceptance, needs retry
                StageStatus::MergeBlocked => blocked += 1,          // Blocked on merge error
                StageStatus::NeedsHumanReview => blocked += 1,      // Waiting for human review
            }
        }

        let mut status_parts = vec![
            format!("{running} running"),
            format!("{pending} pending"),
            format!("{completed} completed"),
        ];

        if blocked > 0 {
            status_parts.push(format!("{blocked} blocked"));
        }

        print!(
            "\r[Polling... {}] (Ctrl+C to detach, 'loom status' for details)    ",
            status_parts.join(", ")
        );
        // Flush stdout to ensure the status line appears immediately
        let _ = io::stdout().flush();
    }
}
