//! Session crash handling and retry logic

use anyhow::Result;
use chrono::Utc;
use std::path::{Path, PathBuf};

use crate::models::failure::FailureInfo;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::retry::{calculate_backoff, should_auto_retry};

use super::crash_classification::{
    is_remote_control_fast_fail, is_startup_refusal, ordinary_crash, startup_refusal_crash,
    CrashClassification, FAST_FAIL_WINDOW_SECS,
};
use super::persistence::Persistence;
use super::{clear_status_line, Orchestrator};

impl Orchestrator {
    /// The stage this crash may act on, or `None` when it must be ignored.
    ///
    /// Three refusals, in order:
    ///
    /// 1. A corrupt/unparseable stage file must not abort the whole daemon
    ///    (O-4) — log and skip; other stages keep running.
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
    fn stage_answerable_for_crash(&self, sid: &str, session_id: &str) -> Option<Stage> {
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
                    "Failed to load stage during crash handling; skipping (corrupt stage file?)"
                );
                return None;
            }
        };

        if matches!(stage.status, StageStatus::Completed) {
            return None;
        }

        if stage.session.as_deref() != Some(session_id) {
            tracing::debug!(
                stage_id = %sid,
                crashed_session = %session_id,
                active_session = ?stage.session,
                "Ignoring crash from a session that is not the stage's active session"
            );
            return None;
        }

        Some(stage)
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

    /// Returns whether THIS call wrote the unsupported marker — i.e. whether
    /// the upcoming retry will spawn with different arguments than the session
    /// that just died. `is_startup_refusal` reads that answer to decide if a
    /// retry is worth making at all.
    fn maybe_disable_remote_control(&self, sid: &str, crashed_session: Option<&Session>) -> bool {
        let Some(session) = crashed_session else {
            return false;
        };
        if is_remote_control_fast_fail(
            (Utc::now() - session.created_at).num_seconds(),
            session.pid.is_some(),
        ) && crate::remote_control::resolve(&self.config.work_dir)
        {
            let _ = crate::remote_control::write_unsupported_marker(&self.config.work_dir);
            clear_status_line();
            eprintln!(
                "Stage '{sid}' crashed within {FAST_FAIL_WINDOW_SECS}s of spawn; \
                 disabling Remote Control for the rest of this run."
            );
            return true;
        }
        false
    }

    /// Read the crash: a startup refusal when claude died before doing any
    /// work, an ordinary (retryable) crash otherwise.
    ///
    /// A `None` session — the daemon restarted since the spawn, so the handle
    /// is gone — leaves no spawn time to measure against the window. No
    /// fast-fail evaluation happens then, exactly as before.
    fn classify_crash(
        &self,
        crashed_session: Option<&Session>,
        crash_report_path: Option<&Path>,
        remote_control_fallback_applied: bool,
    ) -> CrashClassification {
        let refusal = crashed_session.filter(|session| {
            is_startup_refusal(
                (Utc::now() - session.created_at).num_seconds(),
                session.pid.is_some(),
                remote_control_fallback_applied,
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
        // Check if we've already reported this crash to avoid duplicate messages
        if self.reported_crashes.contains(session_id) {
            return Ok(());
        }
        self.reported_crashes.insert(session_id.to_string());

        if let Some(sid) = stage_id {
            let Some(stage) = self.stage_answerable_for_crash(&sid, session_id) else {
                return Ok(());
            };
            // A delayed predecessor crash may name this stage while the map
            // already holds its healthy successor. Remove only the handle
            // whose identity the stage gate above just authorized.
            let crashed_session = self.take_matching_active_session(&sid, session_id);

            // Remote Control fast-fail fallback: `claude --remote-control` exits
            // non-zero when its prerequisites are unmet. If Remote Control is
            // currently active and a session crashed very soon after spawn,
            // treat that as "the flag is unsupported here" — write the
            // `.loom/work/remote_control-unsupported` marker so `resolve()` returns
            // false on the upcoming retry (which omits `--remote-control`).
            // Best-effort: marker write errors are intentionally ignored.
            let fallback_applied =
                self.maybe_disable_remote_control(&sid, crashed_session.as_ref());

            clear_status_line();
            eprintln!("Session '{session_id}' crashed for stage '{sid}'");

            let CrashClassification {
                failure_type,
                reason,
                evidence,
                console_note,
            } = self.classify_crash(
                crashed_session.as_ref(),
                crash_report_path.as_deref(),
                fallback_applied,
            );
            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
            }

            self.sync_crashed_session_permissions(&sid, &stage);

            let detected_at = Utc::now();
            let mut became_terminal = false;
            let updated = self.update_stage(&sid, |current| {
                if current.status == StageStatus::Completed {
                    became_terminal = true;
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
            let updated = match updated {
                Ok(updated) => updated,
                Err(e) => {
                    tracing::error!(
                        stage_id = %sid,
                        error = %e,
                        "Failed to persist Blocked stage after crash; skipping (will retry next tick)"
                    );
                    return Ok(());
                }
            };
            if became_terminal {
                return Ok(());
            }

            let max = updated.max_retries.unwrap_or(3);
            if should_auto_retry(&failure_type, updated.retry_count, max) {
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
                    "Stage '{}' failed after {} attempts. Run `loom diagnose {}` for help.",
                    sid, updated.retry_count, sid
                );
            }

            if let Err(e) = self.graph.mark_status(&sid, StageStatus::Blocked) {
                tracing::warn!(
                    stage_id = %sid,
                    error = %e,
                    "Failed to sync graph status to Blocked after crash"
                );
            }
        } else {
            clear_status_line();
            eprintln!("Session '{session_id}' crashed (no stage association)");
            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "crash_handler_identity_tests.rs"]
mod crash_handler_identity_tests;
