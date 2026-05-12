//! Event handlers for monitor events

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::fs::memory::{
    format_memory_for_handoff, generate_summary, preserve_for_crash, read_journal, write_summary,
};
use crate::handoff::{generate_handoff, HandoffContent};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::orchestrator::continuation::save_session;
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::signals::read_merge_signal;
use crate::orchestrator::spawner::{generate_crash_report, CrashReport};
use crate::orchestrator::terminal::container::logs_capture;
use crate::orchestrator::terminal::container::runtime::Runtime;
use crate::plan::schema::BackendType;

use super::config::MonitorConfig;
use super::context::context_usage_percent;

/// Handler functions for monitor events
pub struct Handlers {
    config: MonitorConfig,
    /// Backend-aware liveness probe. Optional because some test paths
    /// construct `Handlers` without a dispatcher; production paths
    /// always attach one via `Monitor::set_liveness`.
    liveness: Option<LivenessService>,
}

impl Handlers {
    pub fn new(config: MonitorConfig, liveness: Option<LivenessService>) -> Self {
        Self { config, liveness }
    }

    /// Attach (or replace) the backend-aware liveness service.
    pub fn set_liveness(&mut self, liveness: LivenessService) {
        self.liveness = Some(liveness);
    }

    /// Expose work dir for detection logic that needs filesystem access.
    pub fn work_dir(&self) -> &Path {
        &self.config.work_dir
    }

    /// Check if a session is still alive.
    ///
    /// Delegates to the backend-aware [`LivenessService`] when available
    /// (the production path) so container sessions don't fall through to
    /// host-PID checks that would never match. When no liveness service
    /// is attached (test-only construction), returns `Ok(None)` so the
    /// detection loop skips crash reporting for that tick.
    pub fn check_session_alive(&self, session: &Session) -> Result<Option<bool>> {
        let Some(liveness) = self.liveness.as_ref() else {
            return Ok(None);
        };
        match liveness.is_alive(session) {
            Ok(alive) => Ok(Some(alive)),
            Err(_) => Ok(None),
        }
    }

    /// Check if a session is a merge session (has a merge signal file)
    pub fn is_merge_session(&self, session_id: &str) -> bool {
        matches!(
            read_merge_signal(session_id, &self.config.work_dir),
            Ok(Some(_))
        )
    }

    /// Auto-summarize memory at context warning threshold (60%)
    ///
    /// Called when a session reaches the warning threshold.
    /// Generates a summary of the memory journal to reduce context burden.
    pub fn handle_context_warning(&self, session: &Session) -> Result<()> {
        let stage_id = session.stage_id.as_deref().unwrap_or(&session.id);
        let journal = read_journal(&self.config.work_dir, stage_id)?;

        if journal.entries.is_empty() {
            return Ok(());
        }

        // Generate summary (keep last 5 entries for key decisions)
        let summary = generate_summary(&journal, 5);

        // Write summary to the journal
        write_summary(&self.config.work_dir, stage_id, &summary)?;

        eprintln!(
            "Auto-summarized memory for stage '{}' ({} entries)",
            stage_id,
            journal.entries.len()
        );

        Ok(())
    }

    /// Handle critical context by generating a handoff file
    ///
    /// Called when a session reaches critical context threshold.
    /// Loads session and stage data, creates handoff content, and generates the handoff file.
    /// Also merges stage memory into the handoff for continuity.
    pub fn handle_context_critical(&self, session: &Session, stage: &Stage) -> Result<PathBuf> {
        let context_percent = context_usage_percent(session.context_tokens, session.context_limit);

        let goals = stage
            .description
            .clone()
            .unwrap_or_else(|| format!("Work on stage: {}", stage.name));

        // Get memory content for handoff (preserves decisions and questions)
        let stage_id = session.stage_id.as_deref().unwrap_or(&session.id);
        let memory_content = format_memory_for_handoff(&self.config.work_dir, stage_id);

        let content = HandoffContent::new(session.id.clone(), stage.id.clone())
            .with_context_percent(context_percent)
            .with_goals(goals)
            .with_plan_id(stage.plan_id.clone())
            .with_next_steps(vec![
                "Review handoff and continue from current state".to_string()
            ])
            .with_memory_content(memory_content);

        generate_handoff(session, stage, content, &self.config.work_dir)
    }

    /// Handle session crash by generating a crash report
    ///
    /// Called when a session crash is detected.
    /// Creates a CrashReport, generates the crash report file, and preserves stage memory.
    pub fn handle_session_crash(&self, session: &Session, reason: &str) -> Option<PathBuf> {
        let mut report = CrashReport::new(
            session.id.clone(),
            session.stage_id.clone(),
            reason.to_string(),
        );

        // For container sessions, capture the trailing log before the
        // container is removed by `kill_session` cleanup. The runtime
        // binary is persisted on the session at spawn time; fall back to
        // Docker if the field is missing (legacy sessions only).
        if session.backend == BackendType::Container {
            let runtime = session
                .runtime
                .as_deref()
                .and_then(Runtime::from_binary)
                .unwrap_or(Runtime::Docker);
            let container_name = session.container_name.as_deref().unwrap_or("");
            let tail = logs_capture::capture_logs(
                runtime,
                container_name,
                Some(logs_capture::DEFAULT_TAIL),
            )
            .unwrap_or_default();
            if !tail.is_empty() {
                let stage_id_for_log = session.stage_id.as_deref().unwrap_or(&session.id);
                let log_path = logs_capture::persist_log(
                    &self.config.work_dir,
                    stage_id_for_log,
                    &session.id,
                    &tail,
                )
                .ok();
                report = report.with_log_tail(tail);
                if let Some(p) = log_path {
                    report = report.with_log_path(p);
                }
            }
        }

        // Preserve stage memory for recovery
        let stage_id = session.stage_id.as_deref().unwrap_or(&session.id);
        match preserve_for_crash(&self.config.work_dir, stage_id) {
            Ok(Some(path)) => {
                eprintln!("Preserved stage memory: {}", path.display());
            }
            Ok(None) => {
                // No memory to preserve
            }
            Err(e) => {
                eprintln!("Failed to preserve memory for stage '{}': {}", stage_id, e);
            }
        }

        // Generate the crash report
        let crashes_dir = self.config.work_dir.join("crashes");

        match generate_crash_report(&report, &crashes_dir) {
            Ok(path) => {
                eprintln!("Generated crash report: {}", path.display());
                Some(path)
            }
            Err(e) => {
                eprintln!(
                    "Failed to generate crash report for session '{}': {}",
                    session.id, e
                );
                None
            }
        }
    }

    /// Persist session status change to disk immediately
    ///
    /// Called when session status changes are detected (crash, completion, etc.)
    /// to ensure the session file on disk reflects the current state without
    /// waiting for event processing.
    pub fn persist_session_status(&self, session: &Session, new_status: SessionStatus) {
        let mut updated_session = session.clone();
        updated_session.status = new_status;

        if let Err(e) = save_session(&updated_session, &self.config.work_dir) {
            eprintln!(
                "Failed to persist session status for '{}': {}",
                session.id, e
            );
        }
    }
}
