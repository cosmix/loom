//! Server-side handler for `Request::DisputeCriteria`.
//!
//! Trust boundary: the daemon owns dispute persistence. Agents
//! attest to failures by sending the RPC; only the daemon writes
//! `.loom/work/disputes/<stage>/<n>/request.md`, only the daemon
//! transitions the stage to `NeedsAdjudication`, and only the
//! daemon writes `verdict.md` and `applied.marker` (the latter two
//! land in a follow-on stage). The handler:
//!
//! 1. validates the stage id
//! 2. acquires a flock on `.loom/work/disputes/<stage>/.lock`
//! 3. validates every entry of `criterion_indices` against the list `--field`
//!    names (`acceptance`, `wiring` or `wiring_tests`); one filing names
//!    several entries and spends one dispute of the budget
//! 4. refuses if `stage.dispute_budget_exhausted()`
//! 5. refuses if the stage's status cannot move to `NeedsAdjudication`,
//!    checked on a copy before anything is written
//! 6. allocates the next sequential id (`max(existing) + 1`,
//!    starting at 1)
//! 7. creates `.loom/work/disputes/<stage>/<n>/` and writes `request.md`
//!    via the existing `safe_create_new_in_workdir` helper
//! 8. increments `dispute_count`, resets `evidence_rounds = 0`,
//!    transitions the stage to `NeedsAdjudication`, saves the stage
//! 9. responds `Response::DisputeCreated { id }`
//!
//! See `models/dispute.rs` for the on-disk schema; `dispute_store.rs` holds
//! the lock, id allocation, `request.md` write and escalation every dispute
//! kind shares, and `dispute_kinds.rs` files the plan v2 kinds.

use anyhow::Result;
use chrono::Utc;
use std::path::Path;

use super::dispute_store::{escalate_to_human_review, lock_stage_disputes};
use super::Response;
use crate::models::dispute::{
    truncate_to_byte_limit, CriterionField, DisputeKind, DisputeRequest, FAILURE_OUTPUT_MAX_BYTES,
};
use crate::models::stage::Stage;
use crate::verify::transitions::{load_stage, update_stage};

pub fn handle_dispute_criteria(
    work_dir: &Path,
    stage_id: &str,
    field: CriterionField,
    criterion_indices: Vec<usize>,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
) -> Result<Response> {
    handle_dispute_criteria_identified(
        work_dir,
        stage_id,
        (field, criterion_indices),
        reason,
        evidence_commit,
        failure_output,
        None,
    )
}

pub(crate) fn handle_dispute_criteria_identified(
    work_dir: &Path,
    stage_id: &str,
    criterion: (CriterionField, Vec<usize>),
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
    request_id: Option<&str>,
) -> Result<Response> {
    let (field, mut criterion_indices) = criterion;
    // The wire carries any list: the record keeps it sorted and distinct.
    criterion_indices.sort_unstable();
    criterion_indices.dedup();
    if let Some(invalid) = invalid_stage_id(stage_id) {
        return Ok(invalid);
    }

    // Per-stage lock: serialises concurrent filings (id allocation + transition).
    let locked = lock_stage_disputes(work_dir, stage_id)?;
    let work_canonical = &locked.work_dir;

    let stage = load_stage(stage_id, work_canonical)?;
    if let Some(response) = super::receipts::existing(&stage, request_id)? {
        return Ok(response);
    }
    if let Some(refusal) = refusal(&stage, work_canonical, field, &criterion_indices, &reason) {
        return Ok(refusal);
    }

    // `write_request` allocates the next sequential id and creates request.md.
    let dispute_reason = reason.clone();
    let kind = DisputeKind::criterion(field, criterion_indices);
    let record = build_record(&stage, kind, reason, evidence_commit, failure_output);
    let id = super::receipts::write_request(&locked.stage_dir, record, request_id)?;

    // Re-read under the stages-dir lock and mutate only the dispute-owned
    // fields on the fresh on-disk state, so a concurrent write to unrelated
    // fields is not reverted (A-5). `dispute_count` grows from the persisted
    // value; the transition helper handles Executing and CompletedWithFailures.
    update_stage(stage_id, work_canonical, |s| {
        s.dispute_count = s.dispute_count.saturating_add(1);
        super::receipts::remember(s, request_id, id);
        s.tally.evidence_rounds = 0;
        s.try_request_adjudication(Some(dispute_reason))
    })?;

    // The lock releases when `locked` drops.
    drop(locked);
    Ok(Response::DisputeCreated { id })
}

/// The refusal for a malformed `stage_id`, checked BEFORE any FS touch. The
/// handler runs under the daemon RPC trust boundary, but the id arrives
/// unvalidated from the wire: a string like "../../tmp/x" would otherwise force
/// `create_dir_all` to materialise directories outside `.loom/work/disputes/`.
fn invalid_stage_id(stage_id: &str) -> Option<Response> {
    let error = crate::validation::validate_id(stage_id).err()?;
    Some(Response::Error {
        message: format!("invalid stage_id: {error}"),
    })
}

/// The refusal that stops `stage` from filing this dispute, if any: no index,
/// an index outside the field's list, an exhausted budget, or a status that
/// cannot move to `NeedsAdjudication`. Nothing has been written when it
/// returns one.
fn refusal(
    stage: &Stage,
    work_dir: &Path,
    field: CriterionField,
    criterion_indices: &[usize],
    reason: &str,
) -> Option<Response> {
    let len = match field {
        CriterionField::Acceptance => stage.acceptance.len(),
        CriterionField::Wiring => stage.wiring.len(),
        CriterionField::WiringTests => stage.wiring_tests.len(),
    };
    if criterion_indices.is_empty() {
        return Some(Response::Error {
            message: format!("--field {} needs --criterion-index", field.as_str()),
        });
    }
    if let Some(index) = criterion_indices.iter().find(|&&index| index >= len) {
        return Some(Response::Error {
            message: format!(
                "criterion index {index} out of range for --field {} \
                 (stage has {len} entries)",
                field.as_str()
            ),
        });
    }
    if stage.dispute_budget_exhausted() {
        return Some(exhausted_refusal(stage, work_dir));
    }
    transition_refusal(stage, reason)
}

/// The refusal when `stage` cannot move to `NeedsAdjudication`. The transition
/// is checked on a copy: `request.md` is written before the stage is updated, so
/// a refusal found only then would leave an open dispute behind.
pub(super) fn transition_refusal(stage: &Stage, reason: &str) -> Option<Response> {
    let refused = stage
        .clone()
        .try_request_adjudication(Some(reason.to_string()));
    refused.err().map(|error| Response::Error {
        message: format!("cannot dispute stage '{}': {error:#}", stage.id),
    })
}

/// Escalate the stage to human review and refuse: its dispute budget is spent.
fn exhausted_refusal(stage: &Stage, work_dir: &Path) -> Response {
    // The state-machine permits the escalation from both
    // `CompletedWithFailures` (the typical entry point) and
    // `NeedsAdjudication`; if the stage happens to be in some other status
    // we still return the error to the caller and let an operator
    // intervene. The count in the review reason is the fresh on-disk one.
    let count = stage.dispute_count;
    let max = stage.max_disputes_per_stage();
    escalate_to_human_review(&stage.id, work_dir, |s| {
        format!(
            "Dispute budget exhausted ({} of {} disputes filed)",
            s.dispute_count,
            s.max_disputes_per_stage()
        )
    });
    Response::Error {
        message: format!("Dispute budget exhausted ({count} disputes filed; max is {max})."),
    }
}

/// The `request.md` record for a criterion dispute; `write_request` allocates
/// the id.
pub(super) fn build_record(
    stage: &Stage,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
) -> DisputeRequest {
    DisputeRequest {
        id: 0,
        stage_id: stage.id.clone(),
        kind,
        reason,
        evidence_commit,
        // Bound output on a character boundary even for callers that bypass the CLI.
        failure_output: failure_output
            .map(|s| truncate_to_byte_limit(&s, FAILURE_OUTPUT_MAX_BYTES)),
        fix_attempts_at_dispute: stage.fix_attempts,
        created_at: Utc::now(),
    }
}

#[cfg(test)]
#[path = "../daemon/server/dispute_tests.rs"]
mod tests;
