//! Handing a disputed stage to a human.
//!
//! The dispute loop decides THAT a dispute cannot be taken further; the
//! orchestrator decides WHEN the escalation is written. Filing a dispute does
//! not end the filing agent's session, so a stage in `NeedsAdjudication` still
//! names a live, idle agent, and one escalated with that agent in place would
//! be re-adopted by the executor as soon as an operator approved the review.
//! `Orchestrator::check_pending_disputes` retires the agent first and only
//! then calls [`Escalation::write`], which also closes the stage's open
//! disputes (`closed_disputes.rs`).

use chrono::Utc;
use std::path::Path;

use crate::models::failure::{FailureInfo, FailureType};
use crate::models::stage::StageStatus;
use crate::verify::transitions::update_stage;

use super::{close_open_disputes, MAX_EVIDENCE_ROUNDS};

/// A stage the dispute loop hands to a human, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escalation {
    pub stage_id: String,
    /// Recorded as the stage's `review_reason`.
    pub reason: String,
    /// Recorded as the stage's `failure_info`. Only a spawn failure has one;
    /// the evidence, attempt and apply caps exhaust a budget instead.
    pub failure_type: Option<FailureType>,
}

impl Escalation {
    /// No adjudication session could be started for the dispute at all.
    ///
    /// A spawn failure is an environment problem (no terminal, no `claude` on
    /// PATH, a backend that refuses), not something a retry fixes, and a
    /// dispute that hangs silently is worse than one that asks for a human.
    /// `failure_type` is classified the same way a stage or merge-resolver
    /// spawn failure is, so `loom status` shows whether it was a sandbox
    /// setup problem.
    pub(super) fn adjudicator_unavailable(
        stage_id: &str,
        failure_type: FailureType,
        error: &str,
    ) -> Self {
        Self::new(
            stage_id,
            format!("No adjudication session could be started for this dispute: {error}"),
            Some(failure_type),
        )
    }

    pub(super) fn evidence_cap(stage_id: &str) -> Self {
        Self::new(
            stage_id,
            format!("Evidence loop exhausted at {MAX_EVIDENCE_ROUNDS} rounds"),
            None,
        )
    }

    pub(super) fn attempt_cap(stage_id: &str, dispute_id: u32, attempts: u32) -> Self {
        Self::new(
            stage_id,
            format!(
                "Adjudication of dispute {dispute_id} produced no verdict after {attempts} session(s)"
            ),
            None,
        )
    }

    /// A verdict that has failed to apply [`super::MAX_APPLY_ATTEMPTS`]
    /// times. The operator's only other clue is a log line, so the reason
    /// carries the apply error text itself.
    pub(super) fn apply_cap(
        stage_id: &str,
        dispute_id: u32,
        failures: u32,
        error: &anyhow::Error,
    ) -> Self {
        Self::new(
            stage_id,
            format!(
                "Applying the verdict for dispute {dispute_id} failed {failures} time(s): {error:#}"
            ),
            None,
        )
    }

    /// Name in the review reason the disputing agents that could not be
    /// retired, and the command that takes them down.
    pub(crate) fn with_unretired_agents(mut self, detail: &str) -> Self {
        self.reason = format!(
            "{}. The disputing agent was not retired ({detail}); \
             'loom stage reset {} --kill-session' takes it down",
            self.reason, self.stage_id
        );
        self
    }

    fn new(stage_id: &str, reason: String, failure_type: Option<FailureType>) -> Self {
        Self {
            stage_id: stage_id.to_string(),
            reason,
            failure_type,
        }
    }

    /// Move the stage from `NeedsAdjudication` to `NeedsHumanReview`, in a
    /// single locked read-modify-write onto the fresh on-disk stage, then
    /// close every dispute the stage leaves open.
    ///
    /// A stage that has already left `NeedsAdjudication` is left as it is:
    /// its disputing agent is retired only while it holds that status, so
    /// escalating from any other one could hand a human a stage whose agent
    /// is still live. Best effort: a failed write is logged, never fatal.
    pub fn write(&self, work_dir: &Path) {
        let mut escalated = false;
        let result = update_stage(&self.stage_id, work_dir, |stage| {
            if stage.status != StageStatus::NeedsAdjudication {
                return Ok(());
            }
            stage.try_request_human_review(self.reason.clone())?;
            if let Some(failure_type) = &self.failure_type {
                stage.replace_failure(FailureInfo {
                    failure_type: failure_type.clone(),
                    detected_at: Utc::now(),
                    evidence: vec![self.reason.clone()],
                });
            }
            escalated = true;
            Ok(())
        });
        match result {
            Ok(_) if escalated => close_open_disputes(work_dir, &self.stage_id),
            Ok(_) => {}
            Err(error) => tracing::warn!(
                target: "loom::adjudication",
                stage = %self.stage_id,
                %error,
                "failed to escalate stage to NeedsHumanReview",
            ),
        }
    }
}
