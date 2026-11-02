//! Apply a recorded verdict under the dispute claim. The stage transaction
//! stores its consumed identity with counters and status; marker publication
//! and sibling closure can then recover after a crash.
//! `apply_kinds.rs` routes verdicts by dispute kind.

use anyhow::{Context, Result};
use std::os::fd::OwnedFd;
use std::path::Path;

use crate::models::dispute::{applied_marker, dispute_dir, CriterionField, DisputeKind};
use crate::models::stage::{Stage, StageStatus};
use crate::plan::amendment::{apply_amendment, AmendmentRequest};
use crate::verify::transitions::{load_stage, update_stage};

use super::apply_kinds::apply_by_kind;
use super::apply_receipt::{
    apply_reason_change, cap_receipt, finish_apply_cap, publish_applied_marker, verdict_receipt,
    write_marker,
};
use super::closed_disputes::close_dispute_locked;
use super::requeue::requeue_or_hold_for_remaining_disputes;
use super::scan::read_verdict_record;
use super::{
    close_open_disputes_except_locked, feedback, is_closed, lock_stage_dispute_dir,
    resolve_plan_path, AdjudicatorRegistry, MAX_EVIDENCE_ROUNDS,
};

impl AdjudicatorRegistry {
    /// A caller holding `claim` may retire the disputing writer only while
    /// this exact open verdict still owns a stage in adjudication.
    pub(crate) fn verdict_needs_retirement(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
        _claim: &OwnedFd,
    ) -> Result<bool> {
        let root = work_dir.join("disputes");
        let dir = dispute_dir(&root, stage_id, dispute_id);
        if is_closed(&dir) || applied_marker(&root, stage_id, dispute_id).exists() {
            return Ok(false);
        }
        let stage = load_stage(stage_id, work_dir)?;
        Ok(stage.status == StageStatus::NeedsAdjudication
            && !stage
                .tally
                .request_receipts
                .contains_key(&verdict_receipt(dispute_id))
            && !stage
                .tally
                .request_receipts
                .contains_key(&cap_receipt(dispute_id)))
    }

    /// Apply verdict files without an `applied.marker`. The stage receipt is
    /// authoritative after a crash between its write and marker publication.
    pub fn apply_pending_verdicts(&self, work_dir: &Path) -> Result<()> {
        for (stage_id, dispute_id) in self.pending_verdicts(work_dir)? {
            if let Err(e) = self.apply_verdict(work_dir, &stage_id, dispute_id) {
                if e.downcast_ref::<super::counter::CounterError>().is_some() {
                    return Err(e).context("adjudication apply budget unavailable");
                }
                tracing::warn!(
                    target: "loom::adjudication",
                    stage = %stage_id,
                    dispute = dispute_id,
                    error = %e,
                    "failed to apply verdict",
                );
            }
        }
        Ok(())
    }

    /// Apply a single verdict to the stage. Public so tests can drive a
    /// verdict file end-to-end. Wraps `apply_verdict_once` with a
    /// bounded retry: after [`super::MAX_APPLY_ATTEMPTS`] failures the
    /// verdict is a wedge, not a transient, and gets escalated instead.
    pub fn apply_verdict(&self, work_dir: &Path, stage_id: &str, dispute_id: u32) -> Result<()> {
        let lifecycle = lock_stage_dispute_dir(&work_dir.join("disputes").join(stage_id))?;
        self.apply_verdict_claimed(work_dir, stage_id, dispute_id, lifecycle)
    }

    /// Apply while the caller retains the dispute claim across retirement.
    pub(crate) fn apply_verdict_claimed(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
        lifecycle: OwnedFd,
    ) -> Result<()> {
        let dir = dispute_dir(&work_dir.join("disputes"), stage_id, dispute_id);
        if applied_marker(&work_dir.join("disputes"), stage_id, dispute_id).exists()
            || is_closed(&dir)
        {
            return Ok(());
        }
        let receipt = verdict_receipt(dispute_id);
        let current = load_stage(stage_id, work_dir)?;
        let receipts = &current.tally.request_receipts;
        if receipts.contains_key(&cap_receipt(dispute_id)) || receipts.contains_key(&receipt) {
            if current.status == StageStatus::NeedsHumanReview {
                close_open_disputes_except_locked(work_dir, stage_id, Some(dispute_id))?;
            }
            publish_applied_marker(work_dir, stage_id, dispute_id)?;
            return Ok(());
        }
        // No receipt and no adjudication left to answer: an operator or a reset
        // moved the stage, and no later tick can apply this verdict. Closing it
        // ends the retries, charges nothing, and lets the caller retire its judge.
        if current.status != StageStatus::NeedsAdjudication {
            tracing::warn!(target: "loom::adjudication", stage = %stage_id, dispute = dispute_id,
                status = %current.status, "closing a verdict uncharged; the stage moved on");
            return close_dispute_locked(work_dir, stage_id, dispute_id);
        }
        let spent = super::apply_failure_count(work_dir, stage_id, dispute_id)?;
        if spent >= super::MAX_APPLY_ATTEMPTS {
            let error = anyhow::anyhow!("apply attempt budget was exhausted before recovery");
            return finish_apply_cap(work_dir, stage_id, dispute_id, spent, &error, &lifecycle);
        }
        // Reserve durably before any verdict effect. A failed counter write
        // leaves the stage untouched and cannot grant a free retry next tick.
        let attempts = super::reserve_apply_attempt(work_dir, stage_id, dispute_id)?;
        self.apply_reserved_verdict(work_dir, stage_id, dispute_id, attempts, receipt, lifecycle)
    }

    fn apply_reserved_verdict(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
        attempts: u32,
        receipt: String,
        lifecycle: OwnedFd,
    ) -> Result<()> {
        if attempts > super::MAX_APPLY_ATTEMPTS {
            let error = anyhow::anyhow!("apply attempt budget was exhausted concurrently");
            return finish_apply_cap(work_dir, stage_id, dispute_id, attempts, &error, &lifecycle);
        }
        match self.apply_verdict_once(work_dir, stage_id, dispute_id) {
            Ok(()) => Ok(()),
            Err(e) => {
                // The stage transaction can commit successfully even when
                // marker publication fails. That failure is recoverable and
                // must not spend the apply-failure escalation budget.
                let consumed = load_stage(stage_id, work_dir)?
                    .tally
                    .request_receipts
                    .contains_key(&receipt);
                if consumed {
                    return Err(e);
                }
                if attempts >= super::MAX_APPLY_ATTEMPTS {
                    finish_apply_cap(work_dir, stage_id, dispute_id, attempts, &e, &lifecycle)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// One apply attempt, serialized with recording and closing by the dispute lock.
    fn apply_verdict_once(&self, work_dir: &Path, stage_id: &str, dispute_id: u32) -> Result<()> {
        let disputes_root = work_dir.join("disputes");
        let verdict_path =
            crate::models::dispute::verdict_file(&disputes_root, stage_id, dispute_id);
        let applied = applied_marker(&disputes_root, stage_id, dispute_id);
        let dir = dispute_dir(&disputes_root, stage_id, dispute_id);
        if applied.exists() || is_closed(&dir) {
            return Ok(());
        }
        let record = read_verdict_record(&verdict_path)?;
        let request = super::read_request(work_dir, stage_id, dispute_id)
            .with_context(|| format!("read request for stage {stage_id} dispute {dispute_id}"))?;
        let mut stage = load_stage(stage_id, work_dir)?;
        anyhow::ensure!(
            stage.status == StageStatus::NeedsAdjudication,
            "stage left adjudication before dispute {dispute_id} was applied"
        );
        let loaded_reason = stage.review_reason.clone();
        let loaded_updated_at = stage.updated_at;

        apply_by_kind(work_dir, &mut stage, &record, &request.kind)?;
        persist_verdict_result(
            work_dir,
            stage_id,
            dispute_id,
            &stage,
            loaded_reason.as_deref(),
            loaded_updated_at,
            &applied,
        )
    }
}

/// Persist the verdict's stage delta and its receipt in one locked stage write.
///
/// Re-applies ONLY the verdict-owned fields onto the FRESH on-disk stage so a
/// concurrent dispute-thread write (e.g. dispute_count for a parallel filing)
/// or CLI write is not reverted (A-5). The verdict owns: status, the change
/// it made to review_reason (from `loaded_reason`, the reason `stage` was
/// loaded with; see [`apply_reason_change`]), evidence_rounds,
/// and amendments_applied. Amendment policy stays on disk: the amendment
/// transaction already persisted it, and a cached copy could overwrite a
/// newer amendment. The amendment tally is monotonic for the same reason.
///
/// A changed lifecycle refuses the commit. The dispute lock keeps cooperating
/// reset, close and record operations from crossing this boundary.
fn persist_verdict_result(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    stage: &Stage,
    loaded_reason: Option<&str>,
    loaded_updated_at: chrono::DateTime<chrono::Utc>,
    applied: &Path,
) -> Result<()> {
    let receipt = verdict_receipt(dispute_id);
    let verdict_status = stage.status.clone();
    let verdict_evidence_rounds = stage.tally.evidence_rounds;
    let verdict_amendments_applied = stage.tally.amendments_applied;
    update_stage(stage_id, work_dir, |s| {
        if s.tally.request_receipts.contains_key(&receipt) {
            return Ok(());
        }
        anyhow::ensure!(
            s.status == StageStatus::NeedsAdjudication && s.updated_at == loaded_updated_at,
            "stage lifecycle changed while applying dispute {dispute_id}; refusing stale verdict"
        );
        anyhow::ensure!(
            s.tally.request_receipts.len() < 1024,
            "stage receipt capacity exhausted; refusing to lose verdict replay protection"
        );
        if s.status != verdict_status {
            s.try_transition(verdict_status.clone())?;
        }
        apply_reason_change(s, loaded_reason, stage.review_reason.as_deref());
        s.tally.evidence_rounds = verdict_evidence_rounds;
        s.tally.amendments_applied = s.tally.amendments_applied.max(verdict_amendments_applied);
        s.tally.request_receipts.insert(receipt.clone(), dispute_id);
        Ok(())
    })
    .context("save amended stage after verdict apply")?;
    if stage.status == StageStatus::NeedsHumanReview {
        close_open_disputes_except_locked(work_dir, stage_id, Some(dispute_id))?;
    }
    write_marker(applied)?;
    Ok(())
}

/// Apply an accepted verdict's amendment and resync the stage from it.
/// `false` means the amendment cap was reached and the stage escalated
/// instead, so the caller applies nothing more.
pub(super) fn amend_plan(
    work_dir: &Path,
    stage: &mut Stage,
    request: AmendmentRequest,
) -> Result<bool> {
    let plan_path =
        resolve_plan_path(work_dir).ok_or_else(|| anyhow::anyhow!("plan source_path missing"))?;
    match apply_amendment(&plan_path, work_dir, request) {
        Ok(_) => {}
        Err(e) if crate::plan::amendment::is_amendment_cap_error(&e) => {
            escalate_amendment_cap(work_dir, stage, &e);
            return Ok(false);
        }
        Err(e) => return Err(e).context("apply plan amendment from accept verdict"),
    }
    resync_after_amendment(work_dir, stage)?;
    Ok(true)
}

/// Cap exceeded: a further accepted dispute would exceed the per-stage
/// amendment budget. Escalate to human review and let the caller still write
/// applied.marker so we don't loop this verdict forever on the next tick.
fn escalate_amendment_cap(work_dir: &Path, stage: &mut Stage, error: &anyhow::Error) {
    let reason = format!("{error:#}");
    if let Err(transition_err) = stage.try_request_human_review(reason) {
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage.id,
            error = %transition_err,
            "amendment cap exceeded but stage could not be escalated to NeedsHumanReview",
        );
    }
    let _ = feedback::clear_feedback(work_dir, &stage.id);
}

/// Resync the stage's amendable lists from disk (the amendment also rewrites
/// the stage file).
fn resync_after_amendment(work_dir: &Path, stage: &mut Stage) -> Result<()> {
    let reloaded = load_stage(&stage.id, work_dir)?;
    stage.acceptance = reloaded.acceptance;
    stage.wiring = reloaded.wiring;
    stage.wiring_tests = reloaded.wiring_tests;
    stage.contracts = reloaded.contracts;
    stage.harness = reloaded.harness;
    // Derive amendments_applied from the audit log (the source of truth used
    // by the cap check). Bumping the in-memory field by +1 here would
    // double-count on a crash-mid-apply retry: apply_amendment is idempotent
    // (returns the prior result), but the increment-on-reload would have
    // re-bumped each pass.
    stage.tally.amendments_applied =
        crate::plan::amendment::count_amendments_for_stage(work_dir, &stage.id)?;
    Ok(())
}

/// The disputed check stands and the implementation is wrong — the deadlock
/// case.
pub(super) fn apply_reject(
    work_dir: &Path,
    stage: &mut Stage,
    dispute_id: u32,
    reasoning: &str,
    citations: &[crate::models::dispute::Citation],
    kind: &DisputeKind,
) -> Result<()> {
    // The reasoning and citations are what a human needs to arbitrate, so they
    // are still written for the record.
    feedback::append_rejection(work_dir, &stage.id, reasoning, citations)?;
    let verdict_path =
        crate::models::dispute::verdict_file(&work_dir.join("disputes"), &stage.id, dispute_id);
    let reason = format!(
        "Adjudicator upheld the disputed {} (dispute {dispute_id}): \
         it stands and the implementation is what must change. The stage \
         agent judged it impossible, so it cannot make progress unaided — read \
         {} — then decide with: loom stage human-review {}",
        disputed_entry(kind),
        verdict_path.display(),
        stage.id
    );
    // A Reject MUST leave the stage in NeedsHumanReview: re-queueing is the
    // loop this arm exists to break, and leaving it in NeedsAdjudication with
    // an applied verdict would strand it silently. A refused transition
    // (unexpected on-disk drift) is therefore forced rather than propagated.
    if let Err(error) = stage.try_request_human_review(reason.clone()) {
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage.id,
            status = %stage.status,
            %error,
            "reject verdict could not transition the stage to NeedsHumanReview; forcing it",
        );
        stage.force_status_with_reason(StageStatus::NeedsHumanReview, &reason);
        stage.replace_review_reason(reason);
    }
    Ok(())
}

/// What a criterion dispute contested, as `apply_reject`'s review reason names it.
fn disputed_entry(kind: &DisputeKind) -> &'static str {
    match kind {
        DisputeKind::Criterion { field, .. } => match field {
            CriterionField::Acceptance => "acceptance criterion",
            CriterionField::Wiring => "wiring check",
            CriterionField::WiringTests => "wiring test",
        },
        _ => "criterion",
    }
}

/// Undecidable on the evidence supplied: ask the agent the questions, unless
/// the evidence loop is spent.
pub(super) fn apply_needs_more_evidence(
    work_dir: &Path,
    stage: &mut Stage,
    questions: &[String],
) -> Result<()> {
    feedback::append_questions(work_dir, &stage.id, questions)?;
    stage.tally.evidence_rounds = stage.tally.evidence_rounds.saturating_add(1);
    if stage.tally.evidence_rounds >= MAX_EVIDENCE_ROUNDS {
        let reason = format!(
            "Adjudicator evidence loop exhausted ({} rounds)",
            stage.tally.evidence_rounds
        );
        stage.try_request_human_review(reason).ok();
        Ok(())
    } else {
        requeue_or_hold_for_remaining_disputes(work_dir, stage)
    }
}

#[cfg(test)]
#[path = "apply_persistence_tests.rs"]
mod persistence_tests;

#[cfg(test)]
#[path = "apply_recovery_tests.rs"]
mod recovery_tests;
