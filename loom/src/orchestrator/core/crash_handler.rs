//! Session crash handling and retry logic

use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};

use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::retry::{calculate_backoff, should_auto_retry};

use super::crash_classification::{
    is_remote_control_fast_fail, is_startup_refusal, ordinary_crash, startup_refusal_crash,
    CrashClassification, FAST_FAIL_WINDOW_SECS,
};
use super::persistence::Persistence;
use super::{clear_status_line, Orchestrator};

enum CrashTransition {
    Applied(Box<Stage>),
    Stale,
    RetryableFailure,
}

fn crash_applicable(stage: &Stage, session_id: &str) -> bool {
    crate::stage_service::ownership::matches_attempt(stage, session_id)
        && stage.status == StageStatus::Executing
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

    /// Best-effort permission sync before the stage transitions to `Blocked`.
    fn sync_crashed_session_permissions(&self, sid: &str, stage: &Stage) {
        let worktree_path = self.config.repo_root.join(".worktrees").join(sid);
        if !worktree_path.exists() {
            return;
        }
        let working_dir_path = stage.working_dir.as_ref().map(|wd| worktree_path.join(wd));
        match crate::fs::permissions::sync_worktree_permissions_with_working_dir(
            &worktree_path,
            &self.config.repo_root,
            working_dir_path.as_deref(),
        ) {
            Ok(result) if result.allow_added > 0 || result.deny_added > 0 => eprintln!(
                "Synced {} permissions from crashed session for stage '{}'",
                result.allow_added + result.deny_added,
                sid
            ),
            Ok(_) => {}
            Err(e) => eprintln!("Warning: Failed to sync permissions from crashed session: {e}"),
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
            CrashTransition::Applied(updated) => {
                self.take_matching_active_session(sid, session_id);
                self.reported_crashes.insert(session_id.to_string());
                maybe_disable_remote_control(remote_control_suspected);
                self.sync_crashed_session_permissions(sid, &updated);
                clear_status_line();
                eprintln!("Session '{session_id}' crashed for stage '{sid}'");
                if let Some(path) = crash_report_path {
                    eprintln!("Crash report generated: {}", path.display());
                }
            }
            CrashTransition::Stale => {
                self.reported_crashes.insert(session_id.to_string());
            }
            CrashTransition::RetryableFailure => {}
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

/// If this crash is suspected to be a rejected `--remote-control` flag,
/// latch Remote Control off for the rest of this process (see
/// `remote_control::disable_for_this_process`) so the retry drops the flag
/// entirely. A no-op otherwise.
fn maybe_disable_remote_control(remote_control_suspected: bool) {
    if remote_control_suspected {
        crate::remote_control::disable_for_this_process(&format!(
            "session exited within {FAST_FAIL_WINDOW_SECS}s of spawn with --remote-control"
        ));
    }
}

/// The stage-facing note for the crash that triggered the in-process Remote
/// Control disable — names the cause and that the stage will retry without
/// the flag. A no-op for every other crash.
fn note_remote_control_disabled(
    remote_control_suspected: bool,
    sid: &str,
    classification: &mut CrashClassification,
) {
    if !remote_control_suspected {
        return;
    }
    let hint = format!(
        "session exited within {FAST_FAIL_WINDOW_SECS}s with --remote-control; Remote \
         Control disabled for the rest of this daemon run, retrying stage '{sid}' without it"
    );
    classification.reason.push_str(&format!("; {hint}"));
    classification.evidence.push(hint.clone());
    classification.console_note = Some(match classification.console_note.take() {
        Some(note) => format!("{note} {hint}"),
        None => hint,
    });
}

/// The console message for a crash's outcome: the retry countdown, the
/// startup-refusal note, or the "exhausted its attempts" message — whichever
/// applies.
fn announce_crash_outcome(
    sid: &str,
    failure_type: &FailureType,
    updated: &Stage,
    console_note: Option<String>,
) {
    let max = updated.max_retries.unwrap_or(3);
    if should_auto_retry(failure_type, updated.retry_count, max) {
        let backoff = calculate_backoff(updated.retry_count, 30, 300);
        clear_status_line();
        eprintln!(
            "Stage '{}' crashed (attempt {}/{}). Will retry in {}s...",
            sid,
            updated.retry_count,
            max,
            backoff.as_secs()
        );
    } else if let Some(note) = console_note {
        clear_status_line();
        eprintln!("Stage '{sid}': {note}");
    } else if updated.retry_count >= max {
        clear_status_line();
        eprintln!(
            "Stage '{}' failed after {} attempts. Fix the cause, then run `loom stage retry {}`.",
            sid, updated.retry_count, sid
        );
    }
}

#[cfg(test)]
#[path = "crash_handler_identity_tests.rs"]
mod crash_handler_identity_tests;
