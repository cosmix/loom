use anyhow::{Context, Result};
use chrono::Utc;

use crate::context::untrusted::inline_safe;
use crate::fs::session_files::{load_session_exact, mark_session_terminal_reason};
use crate::handoff::{current_blocker, load_trusted_session_checkpoint, short_fingerprint};
use crate::models::session::{Session, SessionExitReason, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::monitor::events::CompletionEscalation;
use crate::orchestrator::signals::remove_signal;
use crate::orchestrator::terminal::native::{session_process_status, SessionProcessStatus};
use crate::subagent_lifecycle::store::{replay, ChildDisposition};

use crate::orchestrator::core::persistence::Persistence;
use crate::orchestrator::core::Orchestrator;

use super::super::recover_hung::HungReport;

impl Orchestrator {
    pub(crate) fn on_completion_blocker(
        &mut self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
        repeat_count: u32,
        escalation: Option<CompletionEscalation>,
    ) -> Result<()> {
        let Some(failure_code) = self.current_completion_failure(
            stage_id,
            session_id,
            fingerprint,
            escalation.as_ref(),
        )?
        else {
            return Ok(());
        };
        if escalation.is_none() {
            eprintln!(
                "Completion blocked for stage '{stage_id}' ({}; verified attempt {repeat_count})",
                short_fingerprint(fingerprint)
            );
            return Ok(());
        }
        match replay(&self.config.work_dir)
            .map(|index| index.session_child_disposition(stage_id, session_id))
            .unwrap_or_else(|error| ChildDisposition::Unknown(error.to_string()))
        {
            ChildDisposition::Active => {
                tracing::warn!(
                    stage_id,
                    session_id,
                    "Active child forbids completion takedown"
                );
                Ok(())
            }
            ChildDisposition::Unknown(reason) => {
                tracing::warn!(stage_id, session_id, %reason, "Child ownership is uncertain");
                self.escalate_completion_ownership(stage_id, session_id, fingerprint)
            }
            ChildDisposition::NoChildren => self.retire_completion_writer(
                stage_id,
                session_id,
                fingerprint,
                repeat_count,
                &failure_code,
            ),
        }
    }

    fn current_completion_failure(
        &self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
        escalation: Option<&CompletionEscalation>,
    ) -> Result<Option<String>> {
        let stage = self.load_stage(stage_id)?;
        if stage.status != StageStatus::Executing || stage.session.as_deref() != Some(session_id) {
            tracing::debug!(
                stage_id,
                session_id,
                "Ignoring stale completion blocker event"
            );
            return Ok(None);
        }
        if matches!(escalation, Some(CompletionEscalation::CapacityExhausted)) {
            return Ok(Some("diagnostic capacity exhausted".into()));
        }
        let checkpoint =
            load_trusted_session_checkpoint(stage_id, session_id, &self.config.work_dir)?;
        let commit = crate::handoff::completion::identity::expected_stage_commit(
            &stage,
            &self.config.repo_root,
        )?;
        let blocker = checkpoint
            .as_ref()
            .and_then(|value| current_blocker(value, &stage, Some(&commit)))
            .filter(|value| value.fingerprint == fingerprint);
        if blocker.is_none() {
            tracing::debug!(
                stage_id,
                session_id,
                "Ignoring stale completion fingerprint"
            );
        }
        Ok(blocker.map(|value| value.external_failure_code.clone()))
    }

    fn retire_completion_writer(
        &mut self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
        repeat_count: u32,
        failure_code: &str,
    ) -> Result<()> {
        let Some(session) = load_session_exact(&self.config.work_dir, session_id)? else {
            return self.escalate_completion_ownership(stage_id, session_id, fingerprint);
        };
        if session_process_status(&self.config.work_dir, &session) == SessionProcessStatus::Missing
        {
            return self.escalate_completion_ownership(stage_id, session_id, fingerprint);
        }
        match self.confirm_session_gone(&session) {
            Ok(true) => self.park_completion(
                stage_id,
                &session,
                fingerprint,
                repeat_count,
                failure_code,
                false,
            ),
            Ok(false) => self.kill_completion_writer(
                stage_id,
                &session,
                fingerprint,
                repeat_count,
                failure_code,
            ),
            Err(error) => {
                tracing::warn!(stage_id, session_id, %error, "Writer liveness is uncertain");
                self.escalate_completion_ownership(stage_id, session_id, fingerprint)
            }
        }
    }

    fn kill_completion_writer(
        &mut self,
        stage_id: &str,
        session: &Session,
        fingerprint: &str,
        repeat_count: u32,
        failure_code: &str,
    ) -> Result<()> {
        match self.take_down_agents(
            stage_id,
            vec![session.clone()],
            SessionExitReason::CriteriaBlocked,
        ) {
            Ok(survivors) if survivors.is_empty() => self.park_completion(
                stage_id,
                session,
                fingerprint,
                repeat_count,
                failure_code,
                true,
            ),
            Ok(_) => self.escalate_completion_ownership(stage_id, &session.id, fingerprint),
            Err(error) => {
                tracing::warn!(stage_id, session_id = %session.id, %error, "Writer takedown is uncertain");
                self.escalate_completion_ownership(stage_id, &session.id, fingerprint)
            }
        }
    }

    fn park_completion(
        &mut self,
        stage_id: &str,
        session: &Session,
        fingerprint: &str,
        repeat_count: u32,
        failure_code: &str,
        session_recorded: bool,
    ) -> Result<()> {
        if !session_recorded {
            mark_session_terminal_reason(
                &self.config.work_dir,
                &session.id,
                SessionStatus::ContextExhausted,
                SessionExitReason::CriteriaBlocked,
            )
            .context("persisting verified completion-blocker retirement")?;
        }
        remove_signal(&session.id, &self.config.work_dir)?;
        self.active_sessions.remove(stage_id);
        let reason = format!(
            "completion blocked {} after {repeat_count} verified attempts: {failure_code}; fix it, then approve, force-complete or reject the review",
            short_fingerprint(fingerprint)
        );
        self.set_completion_review(stage_id, &session.id, reason, true)
    }

    fn escalate_completion_ownership(
        &mut self,
        stage_id: &str,
        session_id: &str,
        fingerprint: &str,
    ) -> Result<()> {
        let reason = format!(
            "completion blocked {}: writer ownership unknown; confirm session {session_id} has exited before reassigning",
            short_fingerprint(fingerprint)
        );
        self.set_completion_review(stage_id, session_id, reason, false)
    }

    fn set_completion_review(
        &mut self,
        stage_id: &str,
        session_id: &str,
        reason: String,
        accumulate_time: bool,
    ) -> Result<()> {
        self.update_stage(stage_id, |stage| {
            ensure_current_writer(stage, session_id)?;
            if accumulate_time {
                stage.accumulate_attempt_time(Utc::now());
            }
            stage.force_status_with_reason(StageStatus::NeedsHumanReview, &reason);
            stage.review_reason = Some(reason.clone());
            Ok(())
        })?;
        self.graph
            .mark_status(stage_id, StageStatus::NeedsHumanReview)
    }
}

fn ensure_current_writer(stage: &Stage, session_id: &str) -> Result<()> {
    anyhow::ensure!(
        stage.status == StageStatus::Executing && stage.session.as_deref() == Some(session_id),
        "stage moved before completion blocker disposition could be persisted"
    );
    Ok(())
}

/// The longest pane line a one-line stall park reason quotes.
const PANE_LINE_CHARS: usize = 120;

/// The longest pane line the review notes keep, so a pane of very wide lines
/// cannot make the stage record unbounded.
const PANE_NOTE_LINE_CHARS: usize = 200;

/// Where the pane's last lines begin in a review reason: see [`with_pane_notes`].
const PANE_NOTES_MARKER: &str = "\n\nLast pane lines:\n";

/// Where the quoted last pane line begins in a one-line reason: see `pane_segment`.
const PANE_QUOTE_MARKER: &str = "; pane: \"";

/// Why a stage that used its automatic stall recoveries was parked.
pub(in crate::orchestrator::core::event_handler) fn exhausted_reason(
    report: &HungReport<'_>,
    recoveries: u32,
    tail: Option<&str>,
) -> String {
    format!(
        "stalled: session {} silent {}s (budget {}s, last: {}) after {recoveries} automatic \
         recoveries{}",
        report.session_id,
        report.stale_duration_secs,
        report.timeout_secs,
        report.last_activity.unwrap_or("none"),
        pane_segment(tail),
    )
}

/// Why a stage whose session never started work was parked.
pub(in crate::orchestrator::core::event_handler) fn never_worked_reason(
    report: &HungReport<'_>,
    tail: Option<&str>,
) -> String {
    format!(
        "session {} never started work (no tool activity {}s after start, budget {}s){}",
        report.session_id,
        report.stale_duration_secs,
        report.timeout_secs,
        pane_segment(tail),
    )
}

/// Why a stage whose session is not logged in was parked, with the remedy.
pub(in crate::orchestrator::core::event_handler) fn not_logged_in_reason(
    session_id: &str,
    stage_id: &str,
) -> String {
    format!(
        "session {session_id} is not logged in to claude in the stage environment; run claude \
         /login (as the operator) and then loom stage human-review {stage_id} --approve"
    )
}

/// The review reason with the pane's last lines below it, each cut to
/// [`PANE_NOTE_LINE_CHARS`]. `Stage` has no notes field; the web view shows
/// these lines as the review notes.
pub(in crate::orchestrator::core::event_handler) fn with_pane_notes(
    reason: String,
    tail: Option<&str>,
) -> String {
    let Some(tail) = tail else {
        return reason;
    };
    let notes: Vec<String> = tail
        .lines()
        .map(|line| line.chars().take(PANE_NOTE_LINE_CHARS).collect())
        .collect();
    format!("{reason}{PANE_NOTES_MARKER}{}", notes.join("\n"))
}

/// The one-line form of a review reason for the daemon log (`orchestrator.log`)
/// and the desktop notification: the reason up to its pane quote and pane notes,
/// flattened to one bounded line. The pane text is agent-controlled, so it
/// stays in the stage's `review_reason` and reaches neither sink. The cut is at
/// the earliest marker, so a marker repeated inside the pane only shortens it.
pub(in crate::orchestrator::core::event_handler) fn review_headline(reason: &str) -> String {
    let end = [PANE_QUOTE_MARKER, PANE_NOTES_MARKER]
        .into_iter()
        .filter_map(|marker| reason.find(marker))
        .min()
        .unwrap_or(reason.len());
    inline_safe(&reason[..end])
}

/// `; pane: "<last line>"` for a one-line reason: the pane's last non-empty
/// line, cut short, its double quotes made single so the quoting stays whole.
/// Nothing at all when no pane was read.
fn pane_segment(tail: Option<&str>) -> String {
    tail.and_then(|tail| {
        tail.lines()
            .rev()
            .map(str::trim)
            .find(|line| !line.is_empty())
    })
    .map(|line| {
        let line: String = line.chars().take(PANE_LINE_CHARS).collect();
        format!("{PANE_QUOTE_MARKER}{}\"", line.replace('"', "'"))
    })
    .unwrap_or_default()
}
