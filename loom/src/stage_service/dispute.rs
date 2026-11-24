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
//! 8. clears feedback on entry to a new round, increments `dispute_count`, resets `evidence_rounds = 0`,
//!    transitions the stage to `NeedsAdjudication`, saves the stage
//! 9. responds `Response::DisputeCreated { id }`
//!
//! Steps 3 to 8 run under the stage lock, on the stage as it is on disk then,
//! and a failed stage write withdraws the request step 7 wrote.
//!
//! See `models/dispute.rs` for the on-disk schema; `dispute_store.rs` holds
//! the locks, id allocation, `request.md` write and escalation every dispute
//! kind shares, and `dispute_kinds.rs` files the plan v2 kinds.

use anyhow::{Context, Result};
use chrono::Utc;
use std::os::fd::AsRawFd;
use std::path::Path;

use super::dispute_store::{file_under_stage_lock, lock_stage_disputes};
use super::Response;
use crate::fs::safe_fs::{safe_open_dirfd, safe_remove_in_workdir};
use crate::models::dispute::{
    feedback_file, truncate_to_byte_limit, CriterionField, CriterionTarget, DisputeKind,
    DisputeRequest, FAILURE_OUTPUT_MAX_BYTES,
};
use crate::models::stage::{Stage, StageStatus, StageType};
#[cfg(test)]
use crate::verify::transitions::update_stage;

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
        CriterionTarget::own(field, criterion_indices),
        reason,
        evidence_commit,
        failure_output,
        None,
    )
}

pub(crate) fn handle_dispute_criteria_identified(
    work_dir: &Path,
    stage_id: &str,
    criterion: CriterionTarget,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
    request_id: Option<&str>,
) -> Result<Response> {
    handle_dispute_criteria_for_session(
        work_dir,
        stage_id,
        criterion,
        reason,
        evidence_commit,
        failure_output,
        (request_id, None),
    )
}

pub(crate) fn handle_dispute_criteria_for_session(
    work_dir: &Path,
    stage_id: &str,
    criterion: CriterionTarget,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
    identity: (Option<&str>, Option<&str>),
) -> Result<Response> {
    let (request_id, session_id) = identity;
    if let Some(invalid) = invalid_stage_id(stage_id) {
        return Ok(invalid);
    }
    let (target, owner_len) = prepare(work_dir, stage_id, criterion);

    // Per-stage lock: serialises concurrent filings (id allocation + transition).
    let locked = lock_stage_disputes(work_dir, stage_id)?;

    #[cfg(test)]
    super::dispute_race_tests::before_stage_update();
    file_under_stage_lock(&locked, stage_id, request_id, |s, publication| {
        if session_id.is_some_and(|id| !super::ownership::matches_attempt(s, id)) {
            return Ok(Response::Error {
                message: "request belongs to a stale stage session".into(),
            });
        }
        if let Some(existing) = super::receipts::existing(s, request_id)? {
            return Ok(existing);
        }
        if let Some(refused) = refusal(s, &target, owner_len.as_ref(), &reason) {
            return Ok(refused);
        }
        #[cfg(test)]
        super::dispute_race_tests::after_admission();
        let kind = DisputeKind::from_target(target.clone());
        let record = build_record(s, kind, reason.clone(), evidence_commit, failure_output);
        let id = publication.write(record)?;
        s.dispute_count = s.dispute_count.saturating_add(1);
        super::receipts::remember(s, request_id, id);
        s.tally.evidence_rounds = 0;
        clear_previous_round_feedback(&locked.work_dir, s)?;
        s.try_request_adjudication(Some(reason))?;
        #[cfg(test)]
        super::dispute_race_tests::after_publication();
        Ok(Response::DisputeCreated { id })
    })
}

/// `target` with its indices sorted and distinct (the wire carries any list),
/// and the owner's wiring length when it names one.
///
/// The owner is read BEFORE the stage lock: `update_stage` holds the stage
/// store lock, and loading another stage inside it would wait on that lock.
/// The owner is a completed stage, so its wiring cannot move under the filing.
fn prepare(
    work_dir: &Path,
    stage_id: &str,
    mut target: CriterionTarget,
) -> (CriterionTarget, Option<Result<usize, String>>) {
    target.criterion_indices.sort_unstable();
    target.criterion_indices.dedup();
    let owner_len = target
        .owner_stage
        .as_deref()
        .map(|owner| owner_wiring_len(work_dir, stage_id, target.field, owner));
    (target, owner_len)
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
/// an owner stage it may not dispute, an index outside the named list, an
/// exhausted budget, or a status that cannot move to `NeedsAdjudication`.
/// Nothing has been written when it returns one.
fn refusal(
    stage: &mut Stage,
    target: &CriterionTarget,
    owner_len: Option<&Result<usize, String>>,
    reason: &str,
) -> Option<Response> {
    let field = target.field;
    if target.criterion_indices.is_empty() {
        return Some(error(format!(
            "--field {} needs --criterion-index",
            field.as_str()
        )));
    }
    let len = match owner_len {
        None => match field {
            CriterionField::Acceptance => stage.acceptance.len(),
            CriterionField::Wiring => stage.wiring.len(),
            CriterionField::WiringTests => stage.wiring_tests.len(),
        },
        Some(_) if stage.stage_type != StageType::IntegrationVerify => {
            return Some(error(format!(
                "stage '{}' is not an integration-verify stage; only one re-verifies another \
                 stage's wiring, so only one disputes it with --owner-stage",
                stage.id
            )));
        }
        Some(Ok(len)) => *len,
        Some(Err(message)) => return Some(error(message.clone())),
    };
    let owner = target
        .owner_stage
        .as_ref()
        .map_or_else(|| "stage".to_string(), |owner| format!("stage '{owner}'"));
    if let Some(index) = target.criterion_indices.iter().find(|&&index| index >= len) {
        return Some(error(format!(
            "criterion index {index} out of range for --field {} ({owner} has {len} entries)",
            field.as_str()
        )));
    }
    if stage.dispute_budget_exhausted() {
        return Some(exhausted_refusal(stage));
    }
    transition_refusal(stage, reason)
}

fn error(message: String) -> Response {
    Response::Error { message }
}

/// The length of `owner`'s wiring list, which the filing stage may dispute
/// only as the integration-verify stage whose aggregated wiring
/// re-verification runs it (checked by [`refusal`]): `owner` is another stage
/// of the plan, already `Completed` and merged, and the field is `wiring`. `Err` is the
/// refusal message.
fn owner_wiring_len(
    work_dir: &Path,
    filing_stage: &str,
    field: CriterionField,
    owner: &str,
) -> Result<usize, String> {
    if field != CriterionField::Wiring {
        return Err(
            "--owner-stage names another stage's wiring list; use it with --field wiring".into(),
        );
    }
    if let Err(invalid) = crate::validation::validate_id(owner) {
        return Err(format!("invalid --owner-stage: {invalid}"));
    }
    if owner == filing_stage {
        return Err(
            "--owner-stage names the filing stage; dispute its own wiring without it".into(),
        );
    }
    let owner_stage = crate::verify::transitions::load_stage(owner, work_dir)
        .map_err(|error| format!("--owner-stage '{owner}': {error:#}"))?;
    if owner_stage.status != StageStatus::Completed {
        return Err(format!(
            "stage '{owner}' is {}, not completed; aggregated wiring re-verification runs only \
             completed stages' wiring",
            owner_stage.status
        ));
    }
    // An accepted verdict amends the owner's wiring, and amending a stage that
    // has not landed clears its `completed_commit`, which the merge needs. An
    // unmerged owner's code is not in the merged tree either, so its gap is
    // the merge's to close, never a wrong check.
    if !(owner_stage.merged || owner_stage.merge_assumed) {
        return Err(format!(
            "stage '{owner}' is completed but not merged; its wiring is disputed only once \
             its work is in the merged tree"
        ));
    }
    Ok(owner_stage.wiring.len())
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

/// Clear the previous round's feedback on entry to adjudication under the stage lock.
pub(super) fn clear_previous_round_feedback(work_dir: &Path, stage: &Stage) -> Result<()> {
    if stage.status == StageStatus::NeedsAdjudication {
        return Ok(());
    }
    let relative = feedback_file(Path::new("disputes"), &stage.id);
    let dirfd = safe_open_dirfd(work_dir)?;
    safe_remove_in_workdir(dirfd.as_raw_fd(), &relative)
        .with_context(|| format!("Failed to remove {}", work_dir.join(&relative).display()))
}

/// Escalate the stage to human review and refuse: its dispute budget is spent.
fn exhausted_refusal(stage: &mut Stage) -> Response {
    // The state-machine permits the escalation from both
    // `CompletedWithFailures` (the typical entry point) and
    // `NeedsAdjudication`; if the stage happens to be in some other status
    // we still return the error to the caller and let an operator
    // intervene. The count in the review reason is the fresh on-disk one.
    let count = stage.dispute_count;
    let max = stage.max_disputes_per_stage();
    let reason = format!("Dispute budget exhausted ({count} of {max} disputes filed)");
    if let Err(error) = stage.try_request_human_review(reason) {
        tracing::warn!(stage = %stage.id, %error, "dispute budget exhausted but stage could not be escalated");
    }
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

#[cfg(test)]
mod feedback_tests {
    use super::*;
    use crate::stage_service::dispute_race_tests::{setup_contract, try_file_contract};
    use std::fs;

    #[test]
    fn only_the_first_filing_of_each_round_clears_feedback() {
        let stage_id = "stage-disp";
        for criterion_first in [true, false] {
            let (_tmp, work_dir) = setup_contract(stage_id);
            let path = feedback_file(&work_dir.join("disputes"), stage_id);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "previous round").unwrap();

            let first = file(&work_dir, stage_id, criterion_first);
            assert!(
                matches!(first, Response::DisputeCreated { id: 1 }),
                "{first:?}"
            );
            assert!(!path.exists());

            fs::write(&path, "current round").unwrap();
            let second = file(&work_dir, stage_id, !criterion_first);
            assert!(
                matches!(second, Response::DisputeCreated { id: 2 }),
                "{second:?}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), "current round");
        }
    }

    fn file(work_dir: &Path, stage_id: &str, criterion: bool) -> Response {
        if criterion {
            handle_dispute_criteria(
                work_dir,
                stage_id,
                CriterionField::Acceptance,
                vec![0],
                "reason".into(),
                None,
                None,
            )
        } else {
            try_file_contract(work_dir, stage_id)
        }
        .unwrap()
    }
}
