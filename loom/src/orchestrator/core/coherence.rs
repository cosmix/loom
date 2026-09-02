//! Per-tick watchdog: an `Executing` stage must name a live worker session of
//! its own kind, repaired or escalated when it does not.
//!
//! `session_lifecycle::adopt_live_session_if_present` only guards against a
//! duplicate spawn at spawn time. A stage can go incoherent while it is
//! already sitting `Executing` — its worker session record disappears, or
//! (before this fix) an adjudication verdict adopts the judge's session into
//! the worker slot — and nothing else runs per tick to notice. This module
//! is that watchdog, called from `start_ready_stages` every pass.

use std::path::Path;

use colored::Colorize;

use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::coherence::{
    block_incoherent_stage, executing_stage_incoherence, load_assigned_session, worker_session_type,
};
use crate::orchestrator::session_registry::live_sessions_for_stage_of_type;

use super::persistence::Persistence;
use super::recovery::{load_stage_at_path, scan_stage_paths, StageScanCounter};
use super::{clear_status_line, Orchestrator};

impl Orchestrator {
    /// Everything that must happen at the start of every scheduling pass,
    /// before ready stages are computed.
    ///
    /// Runs every tick, not just at spawn time: a stage can go incoherent
    /// (e.g. its worker session record disappears) while already sitting
    /// `Executing`, and nothing else polls for that between spawns — so the
    /// watchdog (`reconcile_executing_stages`) runs here on every pass.
    ///
    /// Privileged completion capabilities are command-scoped and must never
    /// cross into an agent runtime, even if the daemon itself was launched
    /// from a shell that happened to carry them — so the runtime strip also
    /// runs here on every pass.
    pub(super) fn begin_scheduling_pass(&mut self) {
        self.reconcile_executing_stages();
        crate::commands::stage::complete::strip_privileged_env_for_runtime();
    }

    /// Every tick: an `Executing` stage must name a live worker session of its
    /// own kind. One that does not is repaired when such a session exists,
    /// and escalated to Blocked otherwise, so it appears in `loom status`
    /// instead of sitting Executing forever with nobody working.
    pub(super) fn reconcile_executing_stages(&mut self) {
        let work_dir = self.config.work_dir.clone();
        let stages_dir = work_dir.join("stages");
        if !stages_dir.exists() {
            return;
        }
        let mut scan = StageScanCounter::default();
        let paths = match scan_stage_paths(&stages_dir, &mut scan) {
            Ok(paths) => paths,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "Failed to enumerate stages while reconciling executing stages; skipping this pass"
                );
                return;
            }
        };

        for path in paths {
            self.reconcile_one_stage_path(&work_dir, &path);
        }
    }

    /// Load one stage file and repair or escalate it if it is an incoherent
    /// `Executing` stage. Any other stage is left untouched.
    fn reconcile_one_stage_path(&mut self, work_dir: &Path, path: &Path) {
        let stage = match load_stage_at_path(path) {
            Ok(stage) => stage,
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    %error,
                    "Failed to load a stage while reconciling executing stages; skipping"
                );
                return;
            }
        };
        if stage.status != StageStatus::Executing {
            return;
        }
        let assigned = match load_assigned_session(work_dir, &stage) {
            Ok(assigned) => assigned,
            Err(error) => {
                // A transient read problem must not read as "no session" and
                // escalate a healthy stage to Blocked; skip this stage this
                // tick and let the next tick retry the read.
                tracing::warn!(
                    stage_id = %stage.id,
                    %error,
                    "Failed to load the session named by an executing stage; skipping this tick"
                );
                return;
            }
        };
        let Some(reason) = executing_stage_incoherence(&stage, assigned.as_ref()) else {
            return;
        };

        let live =
            live_sessions_for_stage_of_type(work_dir, &stage.id, worker_session_type(&stage))
                .unwrap_or_default();
        match live.into_iter().max_by_key(|s| s.created_at) {
            Some(live_session) => self.repair_incoherent_stage(&stage.id, &reason, live_session),
            None => self.escalate_incoherent_stage(work_dir, &stage.id, &reason),
        }
    }

    /// A live worker session exists: re-link the stage to it and start
    /// tracking it, so the stage's own agent is monitored again instead of
    /// the stage sitting linked to a session that was never its worker.
    fn repair_incoherent_stage(&mut self, stage_id: &str, reason: &str, live: Session) {
        let live_id = live.id.clone();
        let live_type = live.session_type;
        if let Err(error) = self.update_stage(stage_id, |s| {
            if s.status == StageStatus::Executing {
                s.assign_session(live_id.clone());
            }
            Ok(())
        }) {
            tracing::warn!(
                stage_id = %stage_id,
                %error,
                "Failed to re-link an incoherent executing stage to its live worker session"
            );
            return;
        }
        if let Some(replaced) = self.active_sessions.insert(stage_id.to_string(), live) {
            if replaced.id != live_id {
                tracing::warn!(
                    stage_id = %stage_id,
                    replaced_session = %replaced.id,
                    new_session = %live_id,
                    "Reconciliation replaced a differently-tracked active session"
                );
            }
        }
        clear_status_line();
        eprintln!(
            "Repaired incoherent Executing stage '{stage_id}': {reason}; re-linked to live {live_type} session '{live_id}'"
        );
    }

    /// No live worker session exists: escalate to Blocked so the stage
    /// appears in `loom status` instead of sitting Executing forever with
    /// nobody working.
    fn escalate_incoherent_stage(&mut self, work_dir: &Path, stage_id: &str, reason: &str) {
        match block_incoherent_stage(work_dir, stage_id, reason) {
            Ok(Some(_)) => {
                let _ = self.graph.mark_status(stage_id, StageStatus::Blocked);
                self.active_sessions.remove(stage_id);
                clear_status_line();
                eprintln!(
                    "{} Stage '{stage_id}' was Executing with nobody working: {reason}. Marked Blocked; fix the cause and 'loom stage retry {stage_id}'.",
                    "INCOHERENT STAGE:".red().bold()
                );
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    stage_id = %stage_id,
                    %error,
                    "Failed to escalate an incoherent executing stage to Blocked"
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "coherence_tests.rs"]
mod coherence_tests;
