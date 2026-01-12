//! Event handlers for monitor events

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::fs::memory::{
    format_memory_for_handoff, generate_summary, preserve_for_crash, read_journal, write_summary,
};
use crate::handoff::{generate_handoff, HandoffContent};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::signals::read_merge_signal;
use crate::orchestrator::spawner::{generate_crash_report, CrashReport};

use super::config::MonitorConfig;
use super::context::context_usage_percent;

/// Handler functions for monitor events
pub struct Handlers {
    config: MonitorConfig,
}

impl Handlers {
    pub fn new(config: MonitorConfig) -> Self {
        Self { config }
    }

    /// Check if a session is still alive by checking its process
    ///
    /// Checks the PID if available (works for both native and terminal sessions).
    pub fn check_session_alive(&self, session: &Session) -> Result<Option<bool>> {
        // Check PID if available (works for both native and terminal sessions)
        if let Some(pid) = session.pid {
            let output = std::process::Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
                .context("Failed to check if process is alive")?;

            if output.status.success() {
                return Ok(Some(true));
            }
            return Ok(Some(false));
        }

        // No PID - cannot track liveness
        Ok(None)
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
        let journal = read_journal(&self.config.work_dir, &session.id)?;

        if journal.entries.is_empty() {
            return Ok(());
        }

        // Generate summary (keep last 5 entries for key decisions)
        let summary = generate_summary(&journal, 5);

        // Write summary to the journal
        write_summary(&self.config.work_dir, &session.id, &summary)?;

        eprintln!(
            "Auto-summarized memory for session '{}' ({} entries)",
            session.id,
            journal.entries.len()
        );

        Ok(())
    }

    /// Handle critical context by generating a handoff file
    ///
    /// Called when a session reaches critical context threshold.
    /// Loads session and stage data, creates handoff content, and generates the handoff file.
    /// Also merges session memory into the handoff for continuity.
    pub fn handle_context_critical(&self, session: &Session, stage: &Stage) -> Result<PathBuf> {
        let context_percent = context_usage_percent(session.context_tokens, session.context_limit);

        let goals = stage
            .description
            .clone()
            .unwrap_or_else(|| format!("Work on stage: {}", stage.name));

        // Get memory content for handoff (preserves decisions and questions)
        let memory_content = format_memory_for_handoff(&self.config.work_dir, &session.id);

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
    /// Creates a CrashReport, generates the crash report file, and preserves session memory.
    pub fn handle_session_crash(&self, session: &Session, reason: &str) -> Option<PathBuf> {
        let report = CrashReport::new(
            session.id.clone(),
            session.stage_id.clone(),
            reason.to_string(),
        );

        // Preserve session memory for recovery
        match preserve_for_crash(&self.config.work_dir, &session.id) {
            Ok(Some(path)) => {
                eprintln!("Preserved session memory: {}", path.display());
            }
            Ok(None) => {
                // No memory to preserve
            }
            Err(e) => {
                eprintln!(
                    "Failed to preserve memory for session '{}': {}",
                    session.id, e
                );
            }
        }

        // Generate the crash report
        let crashes_dir = self.config.work_dir.join("crashes");
        let logs_dir = self.config.work_dir.join("logs");

        match generate_crash_report(&report, &crashes_dir, &logs_dir) {
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
}
