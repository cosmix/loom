//! Applying an accepted criterion verdict: amend every patched entry, then
//! hand the next session the adjudicator's reasoning and a completion attempt
//! as its first move. A narrowing of the acceptance list records the tests it
//! leaves out as hand-offs (`verdict_narrowing`).
//!
//! The session that filed the dispute is retired; the fresh one starts from
//! the signal, whose "Adjudicator Feedback" section is this notice. Without
//! the reasoning a successor cannot tell which criteria were judged, and once
//! assumed the rest had been reviewed and rejected and blocked the stage.

use anyhow::Result;
use std::path::Path;

use crate::models::dispute::{strip_control, truncate_to_byte_limit, PlanPatch};
use crate::models::stage::Stage;
use crate::plan::amendment::{read_audit_rows, AmendmentField, AmendmentPatch, AmendmentRequest};

use super::apply::{amend_plan, apply_needs_more_evidence};
use super::apply_kinds::notify_and_requeue;
use super::feedback::EVIDENCE_ACTION;
use super::plan_patch::build_amendment_requests;
use super::verdict_narrowing::{
    acceptance_json, check_narrowing, narrows_acceptance, NARROWED_FIELD,
};
use crate::verify::review::handoffs::{
    forget_handoffs, load_base_evidence, load_handoffs, record_handoffs, Handoff,
};

/// Most bytes of the adjudicator's reasoning the notice carries.
const REASONING_MAX_BYTES: usize = 2048;

/// The criteria were wrong: amend `amended_stage`'s list (the disputing
/// stage's own, or the completed stage whose wiring an integration-verify
/// stage disputed), one amendment per patch, and re-queue the disputing stage
/// with a notice of what changed and why (unless another dispute is still
/// unanswered).
pub(super) fn apply_accept(
    work_dir: &Path,
    stage: &mut Stage,
    amended_stage: &str,
    plan_patch: &PlanPatch,
    dispute_id: u32,
    reasoning: &str,
) -> Result<()> {
    let key = audit_key(&stage.id, amended_stage, dispute_id);
    let requests = build_amendment_requests(amended_stage, plan_patch, &key)?;
    let notice = accept_notice(&stage.id, dispute_id, &requests, reasoning);
    let handoffs = match narrowing_handoffs(work_dir, stage, amended_stage, plan_patch, &key)? {
        Ok(handoffs) => handoffs,
        Err(reason) => {
            let question = refused_narrowing_question(dispute_id, &reason);
            return apply_needs_more_evidence(work_dir, stage, &[question], EVIDENCE_ACTION);
        }
    };
    if !amend_with_handoffs(work_dir, stage, amended_stage, requests, &handoffs)? {
        return Ok(());
    }
    notify_and_requeue(work_dir, stage, &notice)
}

/// Record `handoffs` under `amended_stage`, then apply `requests`; `false`
/// when the amendment cap escalated the stage instead.
///
/// The hand-offs go first: they are read from the list before it is amended,
/// and a replay after a crash mid-amendment reads the amended list, whose
/// indices may name other entries. `record_handoffs` keeps the hand-off
/// already recorded for `(dispute, field, index)`, so a replay appends
/// nothing. The cap refuses only a verdict's first amendment
/// (`continues_counted_verdict`), so a refused verdict amended nothing and
/// its hand-offs are removed.
pub(super) fn amend_with_handoffs(
    work_dir: &Path,
    stage: &mut Stage,
    amended_stage: &str,
    requests: Vec<AmendmentRequest>,
    handoffs: &[Handoff],
) -> Result<bool> {
    record_handoffs(work_dir, amended_stage, handoffs)?;
    for request in requests {
        if !amend_plan(work_dir, stage, request)? {
            forget_handoffs(work_dir, amended_stage, handoffs)?;
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether an amendment of the verdict keyed `key` (`"<key>.<k>"` per patch)
/// is in the audit log of `stage_id`: a replay of a verdict that already
/// began amending the list.
pub(super) fn verdict_amended(work_dir: &Path, stage_id: &str, key: &str) -> Result<bool> {
    Ok(read_audit_rows(work_dir)?.iter().any(|row| {
        row.stage_id == stage_id
            && row
                .dispute_id
                .as_deref()
                .and_then(|id| id.split('.').next())
                == Some(key)
    }))
}

/// The hand-offs an accepted narrowing of `stage`'s acceptance records. On a
/// replay that finds the list already amended they are the ones recorded
/// before the amendment ([`amend_with_handoffs`]); otherwise they are
/// computed against the unamended list. Recording checked the narrowing on
/// that list, so the inner `Err` (the rule's reason) means it rests on base
/// runs recorded since, or was recorded without the check: the caller then
/// amends nothing, since a criterion is narrowed only on a failing base run
/// with its hand-off recorded.
fn narrowing_handoffs(
    work_dir: &Path,
    stage: &Stage,
    amended_stage: &str,
    plan_patch: &PlanPatch,
    key: &str,
) -> Result<Result<Vec<Handoff>, String>> {
    if amended_stage != stage.id || !narrows_acceptance(&plan_patch.inner) {
        return Ok(Ok(Vec::new()));
    }
    if verdict_amended(work_dir, amended_stage, key)? {
        let mut recorded = load_handoffs(work_dir, amended_stage)?;
        recorded.retain(|handoff| handoff.dispute == key && handoff.field == NARROWED_FIELD);
        return Ok(Ok(recorded));
    }
    let evidence = load_base_evidence(work_dir, amended_stage)?;
    let current = acceptance_json(stage);
    Ok(check_narrowing(
        amended_stage,
        key,
        &plan_patch.inner,
        &current,
        &evidence,
    ))
}

/// The question the disputing agent answers when the accepted narrowing of
/// dispute `dispute_id` no longer checks against the stage's base runs.
fn refused_narrowing_question(dispute_id: u32, reason: &str) -> String {
    format!(
        "Dispute {dispute_id}'s accepted amendment was not applied: it narrows the \
         acceptance list, and the stage's latest base runs no longer support that \
         ({reason}). Show the criterion failing at the stage's base with a fresh \
         `--at-base` run, or fix the stage so the criterion passes."
    )
}

/// The verdict's `dispute_id` in the amendment audit log. `apply_amendment`
/// treats a row with the same stage and dispute id as already applied, and
/// counts the cap per base id, but dispute ids are numbered per FILING stage:
/// a bare `1` from the integration-verify stage would match the owner's own
/// dispute 1 and skip the amendment. Another stage's verdict is therefore
/// keyed `<filing stage>:<id>`; stage ids hold neither `:` nor `.`.
fn audit_key(filing_stage: &str, amended_stage: &str, dispute_id: u32) -> String {
    if filing_stage == amended_stage {
        dispute_id.to_string()
    } else {
        format!("{filing_stage}:{dispute_id}")
    }
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
    let field = match requests.first() {
        Some(request) if request.stage_id != stage_id => {
            format!("stage '{}' {}", request.stage_id, field_name(request.field))
        }
        Some(request) => field_name(request.field).to_string(),
        None => "acceptance".to_string(),
    };
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
        AmendmentField::Reachable => "reachable",
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

#[cfg(test)]
#[path = "apply_accept_replay_tests.rs"]
mod replay_tests;
