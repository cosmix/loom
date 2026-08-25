//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;
use chrono::{DateTime, Utc};
use colored::Colorize;
use std::path::PathBuf;

use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::parked::hung_warning;
use crate::orchestrator::monitor::MonitorEvent;
use crate::orchestrator::signals::remove_signal;

use super::clear_status_line;
use super::persistence::Persistence;
use super::Orchestrator;

fn mark_needs_handoff(stage: &mut Stage, now: DateTime<Utc>) -> Result<()> {
    stage.accumulate_attempt_time(now);
    stage.try_mark_needs_handoff()
}

fn requeue_after_handoff(stage: &mut Stage) -> Result<()> {
    stage.try_mark_queued()
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

    /// Handle budget exceeded (force handoff)
    fn on_budget_exceeded(
        &mut self,
        session_id: &str,
        stage_id: &str,
        usage_percent: f32,
        budget_percent: f32,
    ) -> Result<()>;
}

impl EventHandler for Orchestrator {
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()> {
        for event in events {
            // O-4: one failing event must not drop the rest of the batch or
            // kill the daemon. Each event is handled in isolation; a handler
            // error is logged and the loop moves on to the next event.
            if let Err(e) = self.handle_one_event(event) {
                clear_status_line();
                tracing::error!(error = %e, "Failed to handle monitor event; continuing with next event");
            }
        }
        Ok(())
    }

    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        // Implementation in completion_handler.rs
        self.handle_stage_completed(stage_id)
    }

    fn on_session_crashed(
        &mut self,
        session_id: &str,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    ) -> Result<()> {
        // Implementation in crash_handler.rs
        self.handle_session_crashed(session_id, stage_id, crash_report_path)
    }

    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        clear_status_line();
        eprintln!("Session '{session_id}' needs handoff for stage '{stage_id}'");

        let handoff_at = Utc::now();
        self.update_stage(stage_id, |stage| mark_needs_handoff(stage, handoff_at))?;

        // Kill old session if still tracked
        if let Some(session) = self.active_sessions.get(stage_id) {
            let session_clone = session.clone();
            if let Err(e) = self.backend.kill_session(&session_clone) {
                eprintln!("Warning: Failed to kill session '{session_id}': {e}");
            }
            // Remove old signal file
            if let Err(e) = remove_signal(&session_clone.id, &self.config.work_dir) {
                eprintln!("Warning: Failed to remove signal for session '{session_id}': {e}");
            }
        }
        self.active_sessions.remove(stage_id);

        // Re-queue the stage so the next poll cycle picks it up
        self.update_stage(stage_id, requeue_after_handoff)?;
        self.graph.mark_queued(stage_id)?;

        eprintln!("Stage '{stage_id}' re-queued for continuation after handoff");

        Ok(())
    }

    fn on_merge_session_completed(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        // Implementation in merge_handler.rs
        self.handle_merge_session_completed(session_id, stage_id)
    }

    fn on_budget_exceeded(
        &mut self,
        session_id: &str,
        stage_id: &str,
        usage_percent: f32,
        budget_percent: f32,
    ) -> Result<()> {
        // Implementation in event_handler.rs
        self.handle_budget_exceeded(session_id, stage_id, usage_percent, budget_percent)
    }
}

impl Orchestrator {
    /// Handle a single monitor event. Errors are isolated per event by the
    /// caller (`handle_events`) so one bad event cannot abort the batch or the
    /// daemon (O-4).
    fn handle_one_event(&mut self, event: MonitorEvent) -> Result<()> {
        match event {
            MonitorEvent::StageCompleted { stage_id } => {
                self.on_stage_completed(&stage_id)?;
            }
            MonitorEvent::StageBlocked { stage_id, reason } => {
                clear_status_line();
                eprintln!("Stage '{stage_id}' blocked: {reason}");
                self.graph.mark_status(&stage_id, StageStatus::Blocked)?;
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
            MonitorEvent::SessionHung {
                session_id,
                stage_id,
                stale_duration_secs,
                timeout_secs,
                last_activity,
                finished_without_completing,
            } => {
                clear_status_line();
                // ADVISORY ONLY: nothing is killed and nothing is retried. The
                // wording, and the parked/stuck split, live in monitor::parked.
                let warning = hung_warning(
                    &session_id,
                    stage_id.as_deref(),
                    stale_duration_secs,
                    timeout_secs,
                    last_activity.as_deref(),
                    finished_without_completing,
                );
                eprintln!("{warning}");
            }
            MonitorEvent::HeartbeatReceived {
                stage_id,
                session_id,
                context_percent,
                last_tool: _,
            } => {
                self.apply_heartbeat(&stage_id, &session_id, context_percent)?;
            }
            MonitorEvent::BudgetExceeded {
                session_id,
                stage_id,
                usage_percent,
                budget_percent,
            } => {
                self.on_budget_exceeded(&session_id, &stage_id, usage_percent, budget_percent)?;
            }
            MonitorEvent::StageNeedsHumanReview {
                stage_id,
                review_reason,
            } => {
                clear_status_line();
                let reason_str = review_reason.as_deref().unwrap_or("No reason provided");
                eprintln!(
                    "{} Stage '{}' needs human review: {}",
                    "REVIEW NEEDED:".magenta().bold(),
                    stage_id,
                    reason_str
                );
                crate::orchestrator::notify::notify_needs_human_review(
                    &stage_id,
                    review_reason.as_deref(),
                );
            }
        }
        Ok(())
    }
}

/// Helper to check if a stage is in the ready list of the graph
#[cfg(test)]
fn graph_has_ready_stage(graph: &crate::plan::ExecutionGraph, stage_id: &str) -> bool {
    graph.ready_stages().iter().any(|n| n.id == stage_id)
}

impl Orchestrator {
    /// Handle budget exceeded by generating handoff and transitioning stage
    pub(super) fn handle_budget_exceeded(
        &mut self,
        session_id: &str,
        stage_id: &str,
        usage_percent: f32,
        budget_percent: f32,
    ) -> Result<()> {
        clear_status_line();
        eprintln!(
            "{} Session '{}' exceeded budget: {:.1}% > {:.1}% limit",
            "BUDGET EXCEEDED:".red().bold(),
            session_id,
            usage_percent,
            budget_percent
        );

        // Load the stage
        let stage = self.load_stage(stage_id)?;

        // Get session from active sessions for handoff generation
        if let Some(session) = self.active_sessions.get(stage_id) {
            // Clone session data for handoff generation (avoids borrow conflicts)
            let session_clone = session.clone();

            // Generate handoff using the monitor's context critical handler
            let handoff_path = self
                .monitor
                .handlers()
                .handle_context_critical(&session_clone, &stage)?;

            eprintln!("Generated handoff at: {}", handoff_path.display());
        }

        // Update session status to ContextExhausted and save
        // Clone to avoid borrow conflicts between get_mut and save_session
        if let Some(session_mut) = self.active_sessions.get_mut(stage_id) {
            session_mut.try_mark_context_exhausted()?;
            let session_to_save = session_mut.clone();
            // session_mut goes out of scope here, ending the mutable borrow
            self.save_session(&session_to_save)?;
        }

        let handoff_at = Utc::now();
        self.update_stage(stage_id, |stage| mark_needs_handoff(stage, handoff_at))?;

        // Remove from active sessions
        self.active_sessions.remove(stage_id);

        // Re-queue the stage so the next poll cycle picks it up
        self.update_stage(stage_id, requeue_after_handoff)?;
        self.graph.mark_queued(stage_id)?;

        eprintln!("Stage '{stage_id}' re-queued for continuation after budget exceeded");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Implementers, Stage, StageStatus};
    use crate::plan::schema::{StageDefinition, StageSandboxConfig};
    use crate::plan::ExecutionGraph;
    use crate::verify::transitions::{create_stage, load_stage, update_stage};

    fn create_test_graph() -> ExecutionGraph {
        let stages = vec![StageDefinition {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: None,
            artifacts: vec![],
            wiring: vec![],
            wiring_tests: vec![],
            dead_code_check: None,
            before_stage: vec![],
            after_stage: vec![],
            context_budget: None,
            sandbox: StageSandboxConfig::default(),
            execution_mode: None,
            bug_fix: None,
            regression_test: None,
            model: None,
            reasoning_effort: None,
            code_review: None,
            ultracode: false,
            implementers: Implementers::default(),
            subagent_timeout_secs: None,
        }];
        ExecutionGraph::build(stages).unwrap()
    }

    #[test]
    fn test_needs_handoff_transitions_stage_to_queued() {
        // Verify that the NeedsHandoff -> Queued transition works correctly
        // This is the core logic that on_needs_handoff relies on
        let mut stage = Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            status: StageStatus::Executing,
            ..Stage::default()
        };

        // Transition: Executing -> NeedsHandoff
        stage.try_mark_needs_handoff().unwrap();
        assert_eq!(stage.status, StageStatus::NeedsHandoff);

        // Transition: NeedsHandoff -> Queued (the fix)
        stage.try_mark_queued().unwrap();
        assert_eq!(stage.status, StageStatus::Queued);
    }

    #[test]
    fn test_needs_handoff_requeues_in_graph() {
        // Verify that graph correctly tracks the stage as ready after re-queuing
        let mut graph = create_test_graph();

        // Initially the stage should be ready (WaitingForDeps with no deps = ready)
        assert!(graph_has_ready_stage(&graph, "test-stage"));

        // Mark as executing
        graph.mark_executing("test-stage").unwrap();
        assert!(!graph_has_ready_stage(&graph, "test-stage"));

        // Mark as NeedsHandoff then re-queue
        graph
            .mark_status("test-stage", StageStatus::NeedsHandoff)
            .unwrap();
        graph.mark_queued("test-stage").unwrap();

        // Stage should be ready again for the next poll cycle
        assert!(graph_has_ready_stage(&graph, "test-stage"));
    }

    #[test]
    fn test_budget_exceeded_transitions_to_queued() {
        // Verify the full budget exceeded transition path:
        // Executing -> NeedsHandoff -> Queued
        let mut stage = Stage {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            status: StageStatus::Executing,
            ..Stage::default()
        };

        // Simulate budget exceeded flow
        stage.accumulate_attempt_time(chrono::Utc::now());
        stage.try_mark_needs_handoff().unwrap();
        assert_eq!(stage.status, StageStatus::NeedsHandoff);

        // Re-queue for continuation
        stage.try_mark_queued().unwrap();
        assert_eq!(stage.status, StageStatus::Queued);
    }

    #[test]
    fn handoff_requeue_preserves_concurrent_unrelated_field() {
        use std::sync::{Arc, Barrier};

        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path().to_path_buf();
        let stage = Stage {
            id: "event-race".to_string(),
            name: "Event race".to_string(),
            status: StageStatus::Executing,
            ..Stage::default()
        };
        create_stage(&stage, &work_dir).unwrap();

        let handoff_marked = Arc::new(Barrier::new(2));
        let concurrent_done = Arc::new(Barrier::new(2));
        let event_dir = work_dir.clone();
        let event_marked = Arc::clone(&handoff_marked);
        let event_done = Arc::clone(&concurrent_done);
        let event = std::thread::spawn(move || {
            update_stage("event-race", &event_dir, |stage| {
                mark_needs_handoff(stage, Utc::now())
            })
            .unwrap();
            event_marked.wait();
            event_done.wait();
            update_stage("event-race", &event_dir, requeue_after_handoff).unwrap();
        });

        handoff_marked.wait();
        update_stage("event-race", &work_dir, |stage| {
            stage.dispute_count = 9;
            Ok(())
        })
        .unwrap();
        concurrent_done.wait();
        event.join().unwrap();

        let stage = load_stage("event-race", &work_dir).unwrap();
        assert_eq!(stage.status, StageStatus::Queued);
        assert_eq!(stage.dispute_count, 9);
    }
}
