//! Current-attempt foreground evidence checked before a stall takedown.

use chrono::{DateTime, Utc};

use crate::models::session::Session;
use crate::orchestrator::monitor::foreground::foreground_forward_holds;
use crate::orchestrator::monitor::heartbeat::Heartbeat;
use crate::orchestrator::monitor::never_worked::never_worked;

use super::super::super::Orchestrator;
use super::HungReport;

impl Orchestrator {
    pub(super) fn foreground_stall_facts(
        &mut self,
        stage_id: &str,
        session: Option<&Session>,
        heartbeat: Option<&Heartbeat>,
        report: &HungReport<'_>,
        now: DateTime<Utc>,
    ) -> (bool, bool) {
        let forward = session.and_then(|session| {
            crate::codex_lifecycle::foreground_forward_active(
                &self.config.work_dir,
                stage_id,
                &session.id,
                heartbeat
                    .filter(|heartbeat| heartbeat.session_id == session.id)
                    .map_or(session.created_at, Heartbeat::effective_progress_at),
                std::time::Duration::from_secs(report.timeout_secs),
                now,
            )
        });
        let foreground_held = session.is_some_and(|session| {
            self.monitor
                .foreground_command_holds(session, heartbeat, report.timeout_secs)
                || foreground_forward_holds(
                    &self.config.work_dir,
                    session,
                    heartbeat,
                    report.timeout_secs,
                    now,
                )
        });
        let never_started = session.is_some_and(|session| {
            forward.is_none()
                && !self.monitor.observed_foreground_command(&session.id)
                && never_worked(session, heartbeat, now, report.timeout_secs)
        });
        (never_started, foreground_held)
    }
}
