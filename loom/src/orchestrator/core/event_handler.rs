//! Event handling - processing monitor events and session lifecycle

use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;

use crate::models::stage::StageStatus;
use crate::orchestrator::monitor::MonitorEvent;
use crate::orchestrator::signals::remove_signal;

use super::clear_status_line;
use super::persistence::Persistence;
use super::Orchestrator;

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

        let mut stage = self.load_stage(stage_id)?;
        stage.accumulate_attempt_time(chrono::Utc::now());
        stage.try_mark_needs_handoff()?;
        self.save_stage(&stage)?;

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
        stage.try_mark_queued()?;
        self.save_stage(&stage)?;
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
        let mut stage = self.load_stage(stage_id)?;

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

        // Accumulate execution time before transitioning
        stage.accumulate_attempt_time(chrono::Utc::now());

        // Transition stage to NeedsHandoff
        stage.try_mark_needs_handoff()?;
        self.save_stage(&stage)?;

        // Remove from active sessions
        self.active_sessions.remove(stage_id);

        // Re-queue the stage so the next poll cycle picks it up
        stage.try_mark_queued()?;
        self.save_stage(&stage)?;
        self.graph.mark_queued(stage_id)?;

        eprintln!("Stage '{stage_id}' re-queued for continuation after budget exceeded");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{Stage, StageStatus};
    use crate::plan::schema::{StageSandboxConfig, StageDefinition};
    use crate::plan::ExecutionGraph;

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
            stage_type: crate::plan::schema::StageType::default(),
            truths: vec![],
            artifacts: vec![],
            wiring: vec![],
            truth_checks: vec![],
            wiring_tests: vec![],
            dead_code_check: None,
            before_stage: vec![],
            after_stage: vec![],
            context_budget: None,
            sandbox: StageSandboxConfig::default(),
            execution_mode: None,
            bug_fix: None,
            regression_test: None,
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
}
