//! Session crash handling and retry logic

use anyhow::Result;
use chrono::Utc;
use std::path::PathBuf;

use crate::models::failure::FailureInfo;
use crate::models::stage::StageStatus;
use crate::orchestrator::retry::{calculate_backoff, classify_failure, should_auto_retry};

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
    fn stage_answerable_for_crash(
        &self,
        sid: &str,
        session_id: &str,
    ) -> Option<crate::models::stage::Stage> {
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
            let crashed_session = self.active_sessions.remove(&sid);

            let Some(stage) = self.stage_answerable_for_crash(&sid, session_id) else {
                return Ok(());
            };

            // Remote Control fast-fail fallback: `claude --remote-control` exits
            // non-zero when its prerequisites are unmet. If Remote Control is
            // currently active and a native session crashed very soon after
            // spawn, treat that as "the flag is unsupported here" — write the
            // `.work/remote_control-unsupported` marker so `resolve()` returns
            // false on the upcoming retry (which omits `--remote-control`).
            // Best-effort: marker write errors are intentionally ignored.
            const FAST_FAIL_WINDOW_SECS: i64 = 15;
            if let Some(session) = &crashed_session {
                let crashed_fast =
                    (Utc::now() - session.created_at).num_seconds() <= FAST_FAIL_WINDOW_SECS;
                // A tmux hosting failure must not be misattributed to Remote Control.
                if crashed_fast
                    && session.backend == crate::models::session::SessionBackendKind::Native
                    && crate::remote_control::resolve(&self.config.work_dir)
                {
                    let _ = crate::remote_control::write_unsupported_marker(&self.config.work_dir);
                    clear_status_line();
                    eprintln!(
                        "Stage '{sid}' crashed within {FAST_FAIL_WINDOW_SECS}s of spawn; \
                         disabling Remote Control for the rest of this run."
                    );
                }
            }

            clear_status_line();
            eprintln!("Session '{session_id}' crashed for stage '{sid}'");

            // Classify from a path-FREE reason. The crash-report path embeds
            // `path.display()` (under the user's repo); a repo path containing
            // "merge"/"token" would otherwise reclassify a crash as
            // MergeConflict/ContextExhausted (which `should_auto_retry` rejects),
            // permanently blocking auto-retry. See O-12.
            let classification_reason = "Session crashed";
            let failure_type = classify_failure(classification_reason);

            // Build the human-facing failure reason (may include the path).
            let reason = crash_report_path
                .as_ref()
                .map(|p| format!("Session crashed - see crash report at {}", p.display()))
                .unwrap_or_else(|| classification_reason.to_string());

            if let Some(path) = crash_report_path {
                eprintln!("Crash report generated: {}", path.display());
            }

            // Best-effort permission sync before transitioning to Blocked
            // This preserves permissions granted during the crashed session
            let worktree_path = self.config.repo_root.join(".worktrees").join(&sid);
            if worktree_path.exists() {
                let working_dir_path = stage.working_dir.as_ref().map(|wd| worktree_path.join(wd));
                match crate::fs::permissions::sync_worktree_permissions_with_working_dir(
                    &worktree_path,
                    &self.config.repo_root,
                    working_dir_path.as_deref(),
                ) {
                    Ok(result) => {
                        if result.allow_added > 0 || result.deny_added > 0 {
                            eprintln!(
                                "Synced {} permissions from crashed session for stage '{}'",
                                result.allow_added + result.deny_added,
                                sid
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to sync permissions from crashed session: {e}");
                    }
                }
            }

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
                    evidence: vec![reason.clone()],
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
