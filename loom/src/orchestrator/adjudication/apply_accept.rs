//! Applying an accepted criterion verdict: amend every patched entry, then
//! hand the next session the adjudicator's reasoning and a completion attempt
//! as its first move.
//!
//! The session that filed the dispute is retired; the fresh one starts from
//! the signal, whose "Adjudicator Feedback" section is this notice. Without
//! the reasoning a successor cannot tell which criteria were judged, and once
//! assumed the rest had been reviewed and rejected and blocked the stage.

use anyhow::Result;
use std::path::Path;

use crate::models::dispute::{strip_control, truncate_to_byte_limit, PlanPatch};
use crate::models::stage::Stage;
use crate::plan::amendment::{AmendmentField, AmendmentPatch, AmendmentRequest};

use super::apply::amend_plan;
use super::apply_kinds::notify_and_requeue;
use super::plan_patch::build_amendment_requests;

/// Most bytes of the adjudicator's reasoning the notice carries.
const REASONING_MAX_BYTES: usize = 2048;

/// The criteria were wrong: amend the plan, one amendment per patch, and
/// re-queue the stage with a notice of what changed and why (unless another
/// dispute is still unanswered).
pub(super) fn apply_accept(
    work_dir: &Path,
    stage: &mut Stage,
    plan_patch: &PlanPatch,
    dispute_id: u32,
    reasoning: &str,
) -> Result<()> {
    let requests = build_amendment_requests(&stage.id, plan_patch, dispute_id)?;
    let notice = accept_notice(&stage.id, dispute_id, &requests, reasoning);
    for request in requests {
        if !amend_plan(work_dir, stage, request)? {
            return Ok(());
        }
    }
    notify_and_requeue(work_dir, stage, &notice)
}

/// The feedback the next session reads: what the verdict amended, the
/// adjudicator's reasoning, and the instruction to complete before anything
/// else.
fn accept_notice(
    stage_id: &str,
    dispute_id: u32,
    requests: &[AmendmentRequest],
    reasoning: &str,
) -> String {
    let field = requests
        .first()
        .map_or("acceptance", |r| field_name(r.field));
    let changes = requests
        .iter()
        .rev()
        .map(|request| change(&request.patch))
        .collect::<Vec<_>>()
        .join(", ");
    let mut reasoning = strip_control(reasoning, &['\n', '\t']);
    if reasoning.len() > REASONING_MAX_BYTES {
        reasoning = truncate_to_byte_limit(&reasoning, REASONING_MAX_BYTES) + " [truncated]";
    }
    format!(
        "Dispute {dispute_id} accepted: {field} {changes} (indices as the dispute filed them).\n\n\
         Adjudicator reasoning: {reasoning}\n\n\
         Your committed work stands; run `loom stage complete {stage_id}` before anything \
         else. A criterion still failing is disputed, never blocked.\n"
    )
}

pub(super) fn field_name(field: AmendmentField) -> &'static str {
    match field {
        AmendmentField::Acceptance => "acceptance",
        AmendmentField::Wiring => "wiring",
        AmendmentField::WiringTests => "wiring-tests",
        AmendmentField::Contracts => "contracts",
        AmendmentField::Harness => "harness",
    }
}

/// `13 replaced`, `14 deleted`, `3 inserted`.
fn change(patch: &AmendmentPatch) -> String {
    let verb = match patch {
        AmendmentPatch::Replace { .. } => "replaced",
        AmendmentPatch::Insert { .. } => "inserted",
        AmendmentPatch::Delete { .. } => "deleted",
    };
    format!("{} {verb}", patch.index())
}

#[cfg(test)]
#[path = "apply_accept_tests.rs"]
mod tests;
