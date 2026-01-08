//! Error recovery and state synchronization

use anyhow::Result;
use std::io::{self, Write};

use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::spawner::session_is_running;
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
    fn sync_graph_with_stage_files(&mut self) -> Result<()>;

    /// Recover orphaned sessions (tmux died but session/stage files exist).
    fn recover_orphaned_sessions(&mut self) -> Result<usize>;

    /// Check if all stages are in a terminal state (for watch mode exit)
    fn all_stages_terminal(&self) -> bool;

    /// Print a status update showing current stage counts
    fn print_status_update(&self);
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
            if let Ok(stage) = self.load_stage(stage_id) {
                match stage.status {
                    StageStatus::Completed | StageStatus::Verified => {
                        // Mark as completed in graph (ignore errors for stages not in graph)
                        let _ = self.graph.mark_completed(stage_id);
                    }
                    StageStatus::Executing => {
                        // Will be handled by orphan detection
                    }
                    StageStatus::Blocked => {
                        // Will be handled by orphan recovery
                    }
                    _ => {}
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

            // Check if session has a tmux session name
            let tmux_name = match &session.tmux_session {
                Some(name) => name.clone(),
                None => continue,
            };

            // Check if tmux session is still running
            let is_running = session_is_running(&tmux_name).unwrap_or(false);

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
                            if stage.try_mark_ready().is_err() {
                                // Fallback: directly set status if transition fails
                                stage.status = StageStatus::Ready;
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
                                    let _ = self.graph.mark_ready(stage_id);
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
                NodeStatus::Pending | NodeStatus::Ready | NodeStatus::Executing => {
                    // Need to check the actual stage file for held status
                    if let Ok(stage) = self.load_stage(&node.id) {
                        match stage.status {
                            StageStatus::Verified
                            | StageStatus::Blocked
                            | StageStatus::Completed => {
                                continue;
                            }
                            StageStatus::Ready | StageStatus::Pending if stage.held => continue,
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
                NodeStatus::Pending | NodeStatus::Ready => pending += 1,
                NodeStatus::Completed => completed += 1,
                NodeStatus::Blocked => blocked += 1,
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
