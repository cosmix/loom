//! Session crash handling and retry logic

use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};

use super::crash_classification::{
    is_remote_control_fast_fail, is_startup_refusal, ordinary_crash, startup_refusal_crash,
    CrashClassification,
};
use super::persistence::Persistence;
use super::{clear_status_line, Orchestrator};
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};

#[path = "crash_outcome.rs"]
mod crash_outcome;
use crash_outcome::{
    announce_crash_outcome, maybe_disable_remote_control, note_remote_control_disabled,
};

enum CrashTransition {
    Applied(Box<Stage>),
    Stale,
    RetryableFailure,
}

fn crash_applicable(stage: &Stage, session_id: &str) -> bool {
    crate::stage_service::ownership::matches_attempt(stage, session_id)
        && matches!(
            stage.status,
            StageStatus::Executing | StageStatus::WaitingForInput
        )
}

impl Orchestrator {
    /// The stage this crash may act on, or `None` when it must be ignored.
    ///
    /// Three refusals, in order:
    ///
    /// 1. A corrupt/unparseable stage file must not abort the whole daemon
    ///    (O-4) — return an error for the event loop to isolate and log.
    /// 2. A stage that already reached `Completed` keeps its terminal state;
    ///    the session may simply have died after finishing its work.
    /// 3. **The crash must come from the stage's CURRENT session.** Session
    ///    files accumulate — a stage that crashed and retried leaves the old
    ///    corpse on disk forever — and `reported_crashes` is in-memory, so a
    ///    daemon restart re-observes every historical crash as new. Without
    ///    this those replays are charged to the stage's retry budget and can
    ///    auto-retry a stage whose real session is alive and working, putting
    ///    TWO agents in one worktree: the precise failure `abort_tmux_spawn`
    ///    exists to prevent, arrived by another road. Observed 2026-08-10 in a
    ///    live run, where a session dead for 25 minutes blocked and re-spawned
    ///    a healthy stage the moment the daemon was restarted.
    ///
    /// A crash whose session IS the active one still passes after a restart,
    /// which is what lets a genuinely stranded stage recover.
    fn stage_answerable_for_crash(&self, sid: &str, session_id: &str) -> Result<Option<Stage>> {
        let stage = match self.load_stage(sid) {
            Ok(stage) => stage,
            Err(e) => {
                let path = crate::fs::stage_files::find_stage_file(
                    &self.config.work_dir.join("stages"),
                    sid,
                )
                .ok()
                .flatten();
                clear_status_line();
                tracing::error!(
                    stage_id = %sid,
                    path = ?path,
                    error = %e,
                    "Failed to load stage during crash handling; event remains retryable"
                );
                return Err(e);
            }
        };

        if !crash_applicable(&stage, session_id) {
            tracing::debug!(
                stage_id = %sid,
                crashed_session = %session_id,
                active_session = ?stage.session,
                "Ignoring crash outside the stage's current crash-applicable attempt"
            );
            return Ok(None);
        }

        Ok(Some(stage))
    }

    /// Remove only the in-memory handle that belongs to this crash event.
    fn take_matching_active_session(&mut self, sid: &str, session_id: &str) -> Option<Session> {
        if self
            .active_sessions
            .get(sid)
            .is_some_and(|session| session.id == session_id)
        {
            self.active_sessions.remove(sid)
        } else {
            None
        }
    }

    /// Whether a crashed session's fast, verified-pid exit looks like a
    /// rejected `--remote-control` flag rather than an ordinary failure.
    ///
    /// A `true` verdict drives two things at the call site: it latches
    /// Remote Control off for the rest of this process (see
    /// `maybe_disable_remote_control`), and it makes THIS crash classify as
    /// an ordinary, retryable crash instead of a startup refusal — the
    /// retry that follows drops `--remote-control` entirely.
    fn remote_control_suspected(&self, crashed_session: Option<&Session>) -> bool {
        let Some(session) = crashed_session else {
            return false;
        };
        is_remote_control_fast_fail(
            (Utc::now() - session.created_at).num_seconds(),
            session.pid.is_some(),
        ) && (self.remote_control_active)(&self.config.work_dir)
    }

    /// Read the crash: a startup refusal when claude died before doing any
    /// work, an ordinary (retryable) crash otherwise.
    ///
    /// A `None` session — the daemon restarted since the spawn, so the handle
    /// is gone — leaves no spawn time to measure against the window. No
    /// fast-fail evaluation happens then, exactly as before.
    ///
    /// `remote_control_disabled_now` is the crash's own `remote_control_suspected`
    /// verdict: `true` only for the crash that just latched Remote Control
    /// off for the rest of this process, which reads it as an ordinary,
    /// retryable crash instead of a startup refusal.
    fn classify_crash(
        &self,
        crashed_session: Option<&Session>,
        crash_report_path: Option<&Path>,
        remote_control_disabled_now: bool,
    ) -> CrashClassification {
        let refusal = crashed_session.filter(|session| {
            is_startup_refusal(
                (Utc::now() - session.created_at).num_seconds(),
                session.pid.is_some(),
                remote_control_disabled_now,
            )
        });
        match refusal {
            Some(session) => {
                startup_refusal_crash(&self.config.work_dir, session, crash_report_path)
            }
            None => ordinary_crash(crash_report_path),
        }
    }

    /// Record the approvals the crashed session left behind before the stage
    /// transitions to `Blocked`.
    fn sync_crashed_session_permissions(&self, sid: &str) {
        let recorded = crate::fs::permissions::fold_back_approvals(&self.config.repo_root);
        if recorded > 0 {
            eprintln!(
                "Recorded {recorded} approved permission(s) after the crash of stage '{sid}'"
            );
        }
    }

    pub(super) fn handle_session_crashed(
        &mut self,
        session_id: &str,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    ) -> Result<()> {
        if self.reported_crashes.contains(session_id) {
            return Ok(());
        }
        let Some(sid) = stage_id else {
            self.reported_crashes.insert(session_id.to_string());
            clear_status_line();
            eprintln!("Session '{session_id}' crashed (no stage association)");
            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
            }
            return Ok(());
        };

        if self.stage_answerable_for_crash(&sid, session_id)?.is_none() {
            self.take_matching_active_session(&sid, session_id);
            self.reported_crashes.insert(session_id.to_string());
            return Ok(());
        }
        self.handle_answerable_crash(&sid, session_id, crash_report_path);
        Ok(())
    }

    fn handle_answerable_crash(
        &mut self,
        sid: &str,
        session_id: &str,
        crash_report_path: Option<PathBuf>,
    ) {
        let crashed_session = self
            .active_sessions
            .get(sid)
            .filter(|session| session.id == session_id)
            .cloned();

        // A fast, verified-pid crash while Remote Control is active looks
        // like a rejected `--remote-control` flag; latch it off for this
        // process (see `maybe_disable_remote_control`) so the retry drops it.
        let remote_control_suspected = self.remote_control_suspected(crashed_session.as_ref());
        let mut classification = self.classify_crash(
            crashed_session.as_ref(),
            crash_report_path.as_deref(),
            remote_control_suspected,
        );
        note_remote_control_disabled(remote_control_suspected, sid, &mut classification);
        match self.finish_crash_transition(sid, session_id, classification) {
            CrashTransition::Applied(_) => {
                self.take_matching_active_session(sid, session_id);
                self.reported_crashes.insert(session_id.to_string());
                maybe_disable_remote_control(remote_control_suspected);
                self.sync_crashed_session_permissions(sid);
                clear_status_line();
                eprintln!("Session '{session_id}' crashed for stage '{sid}'");
                if let Some(path) = crash_report_path {
                    eprintln!("Crash report generated: {}", path.display());
                }
            }
            CrashTransition::Stale => {
                self.take_matching_active_session(sid, session_id);
                self.reported_crashes.insert(session_id.to_string());
            }
            // The monitor spaces out the redeliveries, which bounds this line.
            CrashTransition::RetryableFailure => {
                clear_status_line();
                eprintln!(
                    "Stage '{sid}': cannot record the crash of session '{session_id}'; the stage \
                     keeps its slot until its file can be written"
                );
            }
        }
    }

    /// Persist the stage transition to `Blocked` for a classified crash, then
    /// announce the retry (or the reason there is none) and sync the graph.
    fn finish_crash_transition(
        &mut self,
        sid: &str,
        session_id: &str,
        classification: CrashClassification,
    ) -> CrashTransition {
        let CrashClassification {
            failure_type,
            reason,
            evidence,
            console_note,
        } = classification;

        let outcome = self.persist_blocked_crash(sid, session_id, &failure_type, reason, evidence);
        let CrashTransition::Applied(updated) = outcome else {
            return outcome;
        };

        announce_crash_outcome(sid, &failure_type, &updated, console_note);

        if let Err(e) = self.graph.mark_status(sid, StageStatus::Blocked) {
            tracing::warn!(
                stage_id = %sid,
                error = %e,
                "Failed to sync graph status to Blocked after crash"
            );
        }
        CrashTransition::Applied(updated)
    }

    /// Record the crash on the stage and transition it to `Blocked`.
    ///
    /// A fresh owner and crash-applicable status are checked under the stage
    /// lock. A failed read or write leaves the event and active handle retryable.
    /// Not `block_stage`: the transition is conditional and also bumps `retry_count`.
    fn persist_blocked_crash(
        &mut self,
        sid: &str,
        session_id: &str,
        failure_type: &FailureType,
        reason: String,
        evidence: Vec<String>,
    ) -> CrashTransition {
        let detected_at = Utc::now();
        let mut stale = false;
        let updated = self.update_stage(sid, |current| {
            if !crash_applicable(current, session_id) {
                stale = true;
                return Ok(());
            }
            // `WaitingForInput -> Blocked` is no transition edge: end the wait
            // in this same locked update or `try_mark_blocked` refuses.
            if current.status == StageStatus::WaitingForInput {
                crate::verify::contracts::refusal::end_wait(current)?;
            }
            current.accumulate_attempt_time(detected_at);
            current.failure_info = Some(FailureInfo {
                failure_type: failure_type.clone(),
                detected_at,
                evidence,
            });
            current.last_failure_at = Some(detected_at);
            current.retry_count += 1;
            current.close_reason = Some(reason);
            current.try_mark_blocked()
        });
        match updated {
            Ok(_) if stale => CrashTransition::Stale,
            Ok(updated) => CrashTransition::Applied(Box::new(updated)),
            Err(e) => {
                tracing::error!(
                    stage_id = %sid,
                    error = %e,
                    "Failed to persist Blocked stage after crash; retryable on next delivery"
                );
                CrashTransition::RetryableFailure
            }
        }
    }
}

#[cfg(test)]
#[path = "crash_handler_identity_tests.rs"]
mod crash_handler_identity_tests;

#[cfg(test)]
#[path = "crash_redelivery_tests.rs"]
mod crash_redelivery_tests;
