//! Error recovery and state synchronization

use anyhow::Result;
use std::io::{self, Write};

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::retry::{calculate_backoff, is_backoff_elapsed, should_auto_retry};
use crate::parser::frontmatter::parse_from_markdown;
use crate::plan::graph::NodeStatus;

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
                eprintln!(
                    "[sync_graph_with_stage_files] Loaded stage '{}': status={:?}, merged={}",
                    stage.id, stage.status, stage.merged
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
                        eprintln!(
                            "[sync_graph_with_stage_files] Completed stage '{}': merged={}",
                            stage.id, stage.merged
                        );
                        self.graph.set_node_merged(&stage.id, stage.merged);
                        // Now mark as completed - this triggers update_ready_status() which
                        // will see the correct merged value set above
                        let _ = self.graph.mark_completed(&stage.id);
                    }
                    StageStatus::Queued => {
                        // Sync Ready status from stage files to graph
                        // This handles stages marked Ready by `loom verify` -> trigger_dependents()
                        let _ = self.graph.mark_queued(&stage.id);
                    }
                    StageStatus::Executing => {
                        // Mark as executing in graph to track active sessions
                        let _ = self.graph.mark_executing(&stage.id);
                    }
                    StageStatus::Blocked => {
                        // Check if the blocked stage is eligible for automatic retry
                        if check_retry_eligibility(&stage) {
                            // Re-queue the stage for retry
                            if stage.try_mark_queued().is_ok() {
                                clear_status_line();
                                eprintln!(
                                    "Auto-retrying stage '{}' (attempt {})",
                                    stage.id,
                                    stage.retry_count + 1
                                );

                                // ATOMIC UPDATE PATTERN:
                                // 1. Save original graph state for potential rollback
                                // 2. Update graph first (tentatively)
                                // 3. Save file
                                // 4. If save fails, rollback graph to original state
                                let original_graph_status =
                                    self.graph.get_node(&stage.id).map(|n| n.status.clone());

                                // Update graph first
                                let _ = self.graph.mark_queued(&stage.id);

                                // Now save the file
                                if let Err(e) = self.save_stage(&stage) {
                                    eprintln!("Warning: Failed to save stage during retry: {e}");
                                    // Rollback graph to original state
                                    if let Some(NodeStatus::Blocked) = original_graph_status {
                                        let _ = self.graph.mark_blocked(&stage.id);
                                    }
                                }
                            }
                        } else {
                            // Not eligible for retry, just mark as blocked in graph
                            let _ = self.graph.mark_blocked(&stage.id);
                        }
                    }
                    StageStatus::WaitingForInput => {
                        // Stage is paused waiting for user input
                        let _ = self.graph.mark_waiting_for_input(&stage.id);
                    }
                    StageStatus::NeedsHandoff => {
                        // Stage hit context limit and needs handoff to new session
                        let _ = self.graph.mark_needs_handoff(&stage.id);
                    }
                    StageStatus::MergeConflict => {
                        // Stage has merge conflicts that need resolution
                        let _ = self.graph.mark_merge_conflict(&stage.id);
                    }
                    StageStatus::CompletedWithFailures => {
                        // Stage completed but acceptance criteria failed
                        let _ = self.graph.mark_completed_with_failures(&stage.id);
                    }
                    StageStatus::MergeBlocked => {
                        // Stage merge failed with error (not conflicts)
                        let _ = self.graph.mark_merge_blocked(&stage.id);
                    }
                    StageStatus::Skipped => {
                        // Stage was intentionally skipped
                        let _ = self.graph.mark_skipped(&stage.id);
                    }
                    StageStatus::WaitingForDeps => {
                        // Stage is still waiting for dependencies
                        let _ = self.graph.mark_waiting_for_deps(&stage.id);
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
            .filter(|node| node.status == NodeStatus::Queued)
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
                            eprintln!(
                                "Recovering orphaned stage: {} (was {:?})",
                                stage_id, stage.status
                            );

                            // Reset stage to Ready using validated transition
                            // NeedsHandoff -> Queued and Blocked -> Queued are valid transitions
                            // Executing -> Queued is not valid, so we go through Blocked first
                            if stage.status == StageStatus::Executing {
                                // Executing -> Blocked (intermediate step for recovery)
                                if let Err(e) = stage.try_mark_blocked() {
                                    eprintln!("Warning: Failed to transition Executing -> Blocked during recovery: {e}");
                                }
                            }
                            // Now Blocked/NeedsHandoff -> Queued is valid
                            if let Err(e) = stage.try_mark_queued() {
                                // Log a warning that validation was bypassed for recovery
                                eprintln!("Warning: State transition validation failed during orphaned session recovery: {e}");
                                eprintln!(
                                    "         Bypassing validation for recovery (was: {:?})",
                                    stage.status
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
                                if node.status != NodeStatus::Completed {
                                    let _ = self.graph.mark_queued(stage_id);
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
                                        match original_status {
                                            NodeStatus::Executing => {
                                                let _ = self.graph.mark_executing(stage_id);
                                            }
                                            NodeStatus::Blocked => {
                                                let _ = self.graph.mark_blocked(stage_id);
                                            }
                                            NodeStatus::NeedsHandoff => {
                                                let _ = self.graph.mark_needs_handoff(stage_id);
                                            }
                                            _ => {} // Other states unlikely during recovery
                                        }
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
                NodeStatus::Completed => continue,
                NodeStatus::Blocked => continue,
                NodeStatus::Skipped => continue,
                NodeStatus::MergeConflict => continue, // Terminal until resolved
                NodeStatus::CompletedWithFailures => continue, // Terminal until retried
                NodeStatus::MergeBlocked => continue,  // Terminal until fixed
                NodeStatus::WaitingForDeps
                | NodeStatus::Queued
                | NodeStatus::Executing
                | NodeStatus::WaitingForInput
                | NodeStatus::NeedsHandoff => {
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
                NodeStatus::Executing => running += 1,
                NodeStatus::WaitingForDeps | NodeStatus::Queued => pending += 1,
                NodeStatus::Completed => completed += 1,
                NodeStatus::Blocked => blocked += 1,
                NodeStatus::Skipped => completed += 1, // Count skipped as completed for status display
                NodeStatus::WaitingForInput => running += 1, // Paused but still active
                NodeStatus::NeedsHandoff => running += 1, // Needs continuation but still in progress
                NodeStatus::MergeConflict => blocked += 1, // Blocked on conflict resolution
                NodeStatus::CompletedWithFailures => blocked += 1, // Failed acceptance, needs retry
                NodeStatus::MergeBlocked => blocked += 1, // Blocked on merge error
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
