//! Stage completion handling
//!
//! The orchestrator kills the session and runs auto-merge against the
//! host repo directly.

use anyhow::Result;
use chrono::Utc;

use crate::fs::session_files::mark_session_terminal_reason;
use crate::models::session::{SessionExitReason, SessionStatus};
use crate::orchestrator::signals::remove_signal;

use super::persistence::Persistence;
use super::Orchestrator;

impl Orchestrator {
    pub(super) fn handle_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        self.record_completed_stage_session_reason(stage_id);
        self.record_completion_time(stage_id);
        self.retire_completed_stage_session(stage_id);
        self.active_worktrees.remove(stage_id);

        // Merge before advancing the graph: conflicts leave the stage in
        // MergeConflict and must not release dependents.
        if self.try_auto_merge(stage_id) {
            if let Err(e) = self.graph.mark_completed(stage_id) {
                tracing::warn!(
                    stage_id = %stage_id,
                    error = %e,
                    "Failed to mark stage completed in graph; next sync will reconcile"
                );
            }
        }
        Ok(())
    }

    fn record_completion_time(&self, stage_id: &str) {
        // Accumulate execution time for the final attempt. A-4: a corrupt
        // stage file must be logged (with its path), not silently skipped.
        let completed_at = Utc::now();
        if let Err(e) = self.update_stage(stage_id, |stage| {
            stage.accumulate_attempt_time(completed_at);
            Ok(())
        }) {
            let path = crate::fs::stage_files::find_stage_file(
                &self.config.work_dir.join("stages"),
                stage_id,
            )
            .ok()
            .flatten();
            tracing::error!(
                stage_id = %stage_id,
                path = ?path,
                error = %e,
                "Failed to update stage while recording completion time; continuing (corrupt stage file?)"
            );
        }
    }

    fn record_completed_stage_session_reason(&self, stage_id: &str) {
        if let Some(session_id) = self.completed_stage_session_id(stage_id) {
            if let Err(error) = mark_session_terminal_reason(
                &self.config.work_dir,
                &session_id,
                SessionStatus::Completed,
                SessionExitReason::Completed,
            ) {
                tracing::warn!(
                    stage_id = %stage_id,
                    session_id = %session_id,
                    %error,
                    "Failed to record completed session; continuing with merge"
                );
            }
        }
    }

    fn retire_completed_stage_session(&mut self, stage_id: &str) {
        let active_session = self.active_sessions.remove(stage_id);
        if let Some(session) = active_session {
            if let Err(e) = remove_signal(&session.id, &self.config.work_dir) {
                tracing::warn!(
                    stage_id = %stage_id,
                    session_id = %session.id,
                    error = %e,
                    "Failed to remove signal during completion; continuing with merge"
                );
            }
            if let Err(e) = self.backend.kill_session(&session) {
                tracing::warn!(
                    stage_id = %stage_id,
                    session_id = %session.id,
                    error = %e,
                    "Failed to kill session during completion; the agent process may \
                     still be running. Continuing with merge."
                );
            }
        }
    }

    fn completed_stage_session_id(&self, stage_id: &str) -> Option<String> {
        self.active_sessions
            .get(stage_id)
            .map(|session| session.id.clone())
            .or_else(|| match self.load_stage(stage_id) {
                Ok(stage) => stage.session,
                Err(error) => {
                    tracing::warn!(
                        stage_id = %stage_id,
                        %error,
                        "Failed to load completed stage session; continuing with merge"
                    );
                    None
                }
            })
    }
}

#[cfg(test)]
#[path = "knowledge_completion_tests.rs"]
mod knowledge_completion_tests;

#[cfg(test)]
mod terminal_reason_tests {
    use super::*;
    use crate::fs::session_files::{load_session_exact, save_session};
    use crate::models::session::{Session, SessionBackendKind, TerminalConfig};
    use crate::models::stage::{Stage, StageStatus, StageType};
    use crate::orchestrator::core::OrchestratorConfig;
    use crate::plan::ExecutionGraph;
    use crate::verify::transitions::save_stage;

    fn completed_session_fixture() -> (tempfile::TempDir, std::path::PathBuf, Session, Orchestrator)
    {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path().join(".loom").join("work");
        std::fs::create_dir_all(work_dir.join("stages")).unwrap();
        crate::fs::work_dir::write_terminal_config(
            &work_dir,
            &TerminalConfig {
                backend: SessionBackendKind::Tmux,
            },
        )
        .unwrap();
        let mut session = Session::new();
        session.assign_to_stage("notes".to_string());
        session.status = SessionStatus::Running;
        save_session(&session, &work_dir).unwrap();
        let mut stage = Stage::new("notes".to_string(), None);
        stage.id = "notes".to_string();
        stage.status = StageStatus::Completed;
        stage.stage_type = StageType::Knowledge;
        stage.merged = true;
        stage.session = Some(session.id.clone());
        save_stage(&stage, &work_dir).unwrap();
        let config = OrchestratorConfig {
            work_dir: work_dir.clone(),
            repo_root: temp.path().to_path_buf(),
            enable_skill_routing: false,
            ..Default::default()
        };
        let mut orchestrator =
            Orchestrator::new(config, ExecutionGraph::build(Vec::new()).unwrap()).unwrap();
        orchestrator
            .active_sessions
            .insert(stage.id.clone(), session.clone());
        (temp, work_dir, session, orchestrator)
    }

    #[test]
    fn completed_stage_reason_wins_over_a_delayed_crash_update() {
        let (_temp, work_dir, session, mut orchestrator) = completed_session_fixture();

        orchestrator.handle_stage_completed("notes").unwrap();
        mark_session_terminal_reason(
            &work_dir,
            &session.id,
            SessionStatus::Crashed,
            SessionExitReason::Crashed,
        )
        .unwrap();

        let recorded = load_session_exact(&work_dir, &session.id).unwrap().unwrap();
        assert_eq!(
            (recorded.status, recorded.exit_reason),
            (SessionStatus::Completed, Some(SessionExitReason::Completed))
        );
    }
}
