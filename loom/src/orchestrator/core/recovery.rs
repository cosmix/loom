//! Error recovery and state synchronization

use anyhow::Result;
use std::io::{self, Write};

use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::retry::{calculate_backoff, is_backoff_elapsed, should_auto_retry};
use crate::parser::frontmatter::extract_yaml_frontmatter;
use crate::plan::graph::NodeStatus;

use super::persistence::Persistence;
use super::Orchestrator;

/// Clear the current line (status line) before printing a message.
/// This prevents output from being mangled when the status line is being updated.
fn clear_status_line() {
    // \r moves cursor to start of line, \x1B[K clears from cursor to end of line
    print!("\r\x1B[K");
    let _ = io::stdout().flush();
}

/// Trait for recovery operations
pub(super) trait Recovery: Persistence {
    /// Sync the execution graph with existing stage file statuses.
    /// This syncs FROM files TO graph.
    fn sync_graph_with_stage_files(&mut self) -> Result<()>;

    /// Sync queued status from graph back to stage files.
    /// This ensures files reflect when dependencies are satisfied.
    /// This syncs FROM graph TO files.
    fn sync_queued_status_to_files(&mut self) -> Result<()>;

    /// Recover orphaned sessions (tmux died but session/stage files exist).
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
                match stage.status {
                    StageStatus::Completed => {
                        // Mark as completed in graph (ignore errors for stages not in graph)
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
                                // Save the updated stage
                                if let Err(e) = self.save_stage(&stage) {
                                    eprintln!("Warning: Failed to save stage during retry: {e}");
                                } else {
                                    // Update graph to reflect the queued status
                                    let _ = self.graph.mark_queued(&stage.id);
                                }
                            }
                        } else {
                            // Not eligible for retry, just mark as blocked in graph
                            let _ = self.graph.mark_blocked(&stage.id);
                        }
                    }
                    _ => {}
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
            let session: Session = match extract_yaml_frontmatter(&content) {
                Ok(yaml) => match serde_yaml::from_value(yaml) {
                    Ok(s) => s,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };

            // Check if session is still running (works for both native and tmux)
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
                            // NeedsHandoff -> Ready and Blocked -> Ready are valid transitions
                            // Executing -> Ready is not valid, so we go through Blocked first
                            if stage.status == StageStatus::Executing {
                                // Executing -> Blocked -> Ready
                                stage.status = StageStatus::Blocked;
                            }
                            // Now Blocked/NeedsHandoff -> Ready is valid
                            if stage.try_mark_queued().is_err() {
                                // Fallback: directly set status if transition fails
                                stage.status = StageStatus::Queued;
                            }
                            stage.session = None;
                            stage.close_reason = Some("Session crashed/orphaned".to_string());
                            stage.updated_at = chrono::Utc::now();
                            self.save_stage(&stage)?;

                            // Update graph - first ensure it's not in a terminal state
                            if let Some(node) = self.graph.get_node(stage_id) {
                                if node.status != NodeStatus::Completed {
                                    // Try to mark as ready in graph
                                    // This will fail if dependencies aren't satisfied, which is OK
                                    let _ = self.graph.mark_queued(stage_id);
                                }
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
                NodeStatus::WaitingForDeps | NodeStatus::Queued | NodeStatus::Executing => {
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
