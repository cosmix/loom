//! Server-side handler for `Request::FileDispute` (DESIGN D15): a plan v2
//! stage disputes review findings, a frozen contract, test-integrity events
//! or failing impact-selected tests.
//!
//! It files under the same per-stage lock, id allocation and create-new
//! `request.md` as `handle_dispute_criteria` (`dispute_store.rs`), after checks
//! of its own:
//!
//! 1. the stage is a plan v2 stage: a v1 stage disputes only its criteria;
//! 2. every id names something loom itself recorded or derives now: an open
//!    finding (own or carried), a frozen contract, a current test-integrity
//!    event, or an impact-test failure in the stage's observed record
//!    (`reviews/<stage>/impact-observed.json`, which only loom's gate run
//!    writes, `verify::impact_tests::observed`). The evidence `request.md`
//!    records is the daemon's own copy of those ids, never the client's;
//! 3. the kind's budget (`models::stage::dispute_budgets`); an exhausted one
//!    escalates the stage to NeedsHumanReview and files nothing.
//!
//! The reason is the agent's own text, kept to `DISPUTE_REASON_MAX_BYTES`.
//!
//! A refusal is a `Response::Error`, never an `Err`: the spool drain retries
//! an `Err` on every tick, and a dispute refused once is refused forever.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::dispute::transition_refusal;
use super::dispute_store::{escalate_to_human_review, lock_stage_disputes};
use super::Response;
use crate::models::dispute::{
    select_events, select_findings, select_impact, truncate_to_byte_limit, DisputeKind,
    DISPUTE_REASON_MAX_BYTES,
};
use crate::models::stage::dispute_budgets::{self, MAX_DISPUTES_PER_KIND};
use crate::models::stage::{Stage, StageType};
use crate::verify::contracts::store::load_freeze;
use crate::verify::impact_tests::{check_failure_id, load_observed};
use crate::verify::review::store::open_findings;
use crate::verify::transitions::{load_stage, update_stage};

pub(crate) fn handle_file_dispute(
    work_dir: &Path,
    stage_id: &str,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
) -> Result<Response> {
    handle_file_dispute_identified(work_dir, stage_id, kind, reason, evidence_commit, None)
}

pub(crate) fn handle_file_dispute_identified(
    work_dir: &Path,
    stage_id: &str,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
    request_id: Option<&str>,
) -> Result<Response> {
    // The stage id arrives unvalidated from the wire and names directories.
    if let Err(error) = crate::validation::validate_id(stage_id) {
        return Ok(refused(format!("invalid stage_id: {error}")));
    }
    let reason = truncate_to_byte_limit(&reason, DISPUTE_REASON_MAX_BYTES);
    let locked = lock_stage_disputes(work_dir, stage_id)?;
    let stage = load_stage(stage_id, &locked.work_dir)?;
    if let Some(response) = super::receipts::existing(&stage, request_id)? {
        return Ok(response);
    }
    let kind = match confirm(&locked.work_dir, &stage, kind) {
        Ok(kind) => kind,
        Err(error) => return Ok(refused(format!("{error:#}"))),
    };
    if dispute_budgets::exhausted(&stage, &kind) {
        return Ok(escalate_exhausted(&locked.work_dir, &stage, &kind));
    }
    if let Some(refusal) = transition_refusal(&stage, &reason) {
        return Ok(refusal);
    }

    let record =
        super::dispute::build_record(&stage, kind.clone(), reason.clone(), evidence_commit, None);
    let id = super::receipts::write_request(&locked.stage_dir, record, request_id)?;

    // Spend the budget and transition on the fresh on-disk stage, as the
    // criterion path does (A-5).
    update_stage(stage_id, &locked.work_dir, |s| {
        dispute_budgets::spend(s, &kind);
        super::receipts::remember(s, request_id, id);
        s.tally.evidence_rounds = 0;
        s.try_request_adjudication(Some(reason))
    })?;
    drop(locked);
    Ok(Response::DisputeCreated { id })
}

fn refused(message: String) -> Response {
    Response::Error { message }
}

/// Escalate to NeedsHumanReview and refuse, as the criterion path does when
/// its budget is spent.
fn escalate_exhausted(work_dir: &Path, stage: &Stage, kind: &DisputeKind) -> Response {
    let name = kind.name();
    escalate_to_human_review(&stage.id, work_dir, |fresh| {
        format!(
            "Dispute budget exhausted ({} of {MAX_DISPUTES_PER_KIND} {name} disputes filed)",
            dispute_budgets::filed(fresh, kind)
        )
    });
    refused(format!(
        "Dispute budget exhausted ({} {name} disputes filed; max is {MAX_DISPUTES_PER_KIND}).",
        dispute_budgets::filed(stage, kind)
    ))
}

/// The dispute to record: `kind` with every id checked against what exists
/// now and its evidence re-derived by the daemon.
fn confirm(work_dir: &Path, stage: &Stage, kind: DisputeKind) -> Result<DisputeKind> {
    if stage.plan_version != 2 {
        bail!(
            "stage '{}' is not a plan version 2 stage; only its acceptance criteria, wiring \
             checks and wiring tests can be disputed, with `loom stage dispute-criteria --field`",
            stage.id
        );
    }
    match kind {
        DisputeKind::Criterion { .. } => {
            bail!("a criterion is disputed with `loom stage dispute-criteria`")
        }
        DisputeKind::Findings { finding_ids, .. } => {
            let open = open_findings(work_dir, &stage.id)?;
            let evidence = select_findings(&open, &finding_ids)?;
            Ok(DisputeKind::Findings {
                finding_ids,
                evidence,
            })
        }
        DisputeKind::Contract { contract_id } => {
            check_frozen(work_dir, &stage.id, &contract_id)?;
            Ok(DisputeKind::Contract { contract_id })
        }
        DisputeKind::Integrity { event_ids, .. } => {
            let events = super::observer::integrity_events(work_dir, stage)?;
            let evidence = select_events(&events, &event_ids)?;
            Ok(DisputeKind::Integrity {
                event_ids,
                evidence,
            })
        }
        // Whatever evidence the client sent is dropped: the observed record is
        // it, the stage session's report of its latest failing completion.
        DisputeKind::ImpactTests { test_ids, .. } => confirm_impact(work_dir, stage, test_ids),
    }
}

/// An impact-test dispute of a standard stage, its evidence each named
/// failure as the observed record holds it (an unauthenticated report the
/// adjudicator checks by re-running the selection).
fn confirm_impact(work_dir: &Path, stage: &Stage, test_ids: Vec<String>) -> Result<DisputeKind> {
    if stage.stage_type != StageType::Standard {
        bail!(
            "stage '{}' is not a standard stage; impact-selected tests run only when a \
             standard stage completes",
            stage.id
        );
    }
    for id in &test_ids {
        check_failure_id(id)?;
    }
    let observed = load_observed(work_dir, &stage.id)?;
    let evidence = select_impact(&observed.failures, &test_ids).with_context(|| {
        format!(
            "loom keeps only the failures the latest failing `loom stage complete {}` run \
             reported; dispute an id that run listed as disputable",
            stage.id
        )
    })?;
    Ok(DisputeKind::ImpactTests { test_ids, evidence })
}

fn check_frozen(work_dir: &Path, stage_id: &str, contract_id: &str) -> Result<()> {
    let Some(record) = load_freeze(work_dir, stage_id)? else {
        bail!("the contracts of stage '{stage_id}' are not frozen");
    };
    if !record
        .contracts
        .iter()
        .any(|frozen| frozen.id == contract_id)
    {
        bail!("'{contract_id}' names no frozen contract of this stage");
    }
    Ok(())
}

#[cfg(test)]
#[path = "../daemon/server/dispute_kinds_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../daemon/server/dispute_impact_tests.rs"]
mod impact_tests;
