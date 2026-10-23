//! Server-side handler for `Request::FileDispute` (DESIGN D15): a plan v2
//! stage disputes review findings, a frozen contract, test-integrity events
//! or failing impact-selected tests; and loom's own filing of a refused
//! contract freeze ([`file_freeze_refusal`]).
//!
//! It files under the same per-stage lock, id allocation and create-new
//! `request.md` as `handle_dispute_criteria` (`dispute_store.rs`), after checks
//! of its own:
//!
//! 1. the stage is a plan v2 stage: a v1 stage disputes only its criteria;
//! 2. the stage's current session may file the kind (`DisputeKind::filed_by`):
//!    the session record `stage.session` names is the filer, because every
//!    transport admits only the stage's owner. A stage with no current session
//!    (an operator's filing) admits every kind but a freeze refusal;
//! 3. every id names something loom itself recorded or derives now: an open
//!    finding (own or carried), a frozen contract, a current test-integrity
//!    event, or an impact-test failure in the stage's observed record
//!    (`reviews/<stage>/impact-observed.json`, which only loom's gate run
//!    writes, `verify::impact_tests::observed`). The evidence `request.md`
//!    records is the daemon's own copy of those ids, never the client's. A
//!    freeze refusal names an unfrozen contract phase instead;
//! 4. the kind's budget (`models::stage::dispute_budgets`); an exhausted one
//!    escalates the stage to NeedsHumanReview and files nothing.
//!
//! The reason is the agent's own text, kept to `DISPUTE_REASON_MAX_BYTES`.
//!
//! A refusal is a `Response::Error`, never an `Err`: the spool drain retries
//! an `Err` on every tick, and a dispute refused once is refused forever. The
//! transport entries refuse a freeze refusal outright: loom alone files one.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::dispute::transition_refusal;
use super::dispute_store::{escalate_to_human_review, lock_stage_disputes};
use super::Response;
use crate::context::untrusted::inline_safe;
use crate::fs::session_files::load_session_exact;
use crate::models::dispute::{
    select_events, select_findings, select_impact, truncate_to_byte_limit, DisputeKind,
    DISPUTE_REASON_MAX_BYTES,
};
use crate::models::stage::dispute_budgets::{self, MAX_DISPUTES_PER_KIND};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::contracts::store::load_freeze;
use crate::verify::impact_tests::{check_failure_id, load_observed};
use crate::verify::review::store::open_findings;
use crate::verify::transitions::{load_stage, update_stage};

/// The refusal every transport answers a freeze refusal with.
const LOOM_ONLY: &str = "freeze-refusal disputes are filed by loom only";

/// Most problems, and most bytes of each, a freeze-refusal dispute keeps.
const MAX_REFUSAL_PROBLEMS: usize = 32;
const MAX_PROBLEM_BYTES: usize = 1024;

/// What [`file_freeze_refusal`] did: filed the dispute, or declined it with
/// the admission's reason. A declined refusal is never retried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FreezeRefusalFiling {
    Filed(String),
    Declined(String),
}

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
    if matches!(kind, DisputeKind::FreezeRefusal { .. }) {
        return Ok(refused(LOOM_ONLY.to_string()));
    }
    file_dispute_inner(
        work_dir,
        stage_id,
        kind,
        reason,
        evidence_commit,
        request_id,
    )
}

/// File loom's own dispute over a refused contract freeze of `stage_id`.
/// `request_key` names the refusal, so a retried filing reuses one
/// `request.md`. `Err` only when the filing could not be read or written.
pub(crate) fn file_freeze_refusal(
    work_dir: &Path,
    stage_id: &str,
    problems: Vec<String>,
    request_key: &str,
) -> Result<FreezeRefusalFiling> {
    let problems = normalized_problems(&problems);
    let first = problems
        .first()
        .map_or("no problem was recorded", String::as_str);
    let reason = format!("contract freeze refused: {first}");
    let kind = DisputeKind::FreezeRefusal { problems };
    let key = receipt_key(request_key);
    Ok(
        match file_dispute_inner(work_dir, stage_id, kind, reason, None, Some(&key))? {
            Response::DisputeCreated { id } => {
                FreezeRefusalFiling::Filed(format!("freeze-refusal dispute {id} filed"))
            }
            Response::Error { message } => FreezeRefusalFiling::Declined(message),
            other => FreezeRefusalFiling::Declined(format!("unexpected filing outcome {other:?}")),
        },
    )
}

/// Send `stage_id` to a human once its refused freezes have spent the
/// freeze-refusal budget. The caller has retired the writer already;
/// `unretired` names a survivor of that retirement.
pub(crate) fn escalate_freeze_refusal(
    work_dir: &Path,
    stage_id: &str,
    problems: &[String],
    unretired: Option<&str>,
) -> Result<()> {
    crate::validation::validate_id(stage_id)?;
    let last = normalized_problems(problems).join("; ");
    let mut reason = format!(
        "contract freeze refused {} times; adjudication could not resolve it: {}. Decide \
         with: loom stage human-review {stage_id}",
        MAX_DISPUTES_PER_KIND + 1,
        truncate_to_byte_limit(&last, DISPUTE_REASON_MAX_BYTES)
    );
    if let Some(detail) = unretired {
        reason.push_str(&format!(
            ". The contract writer was not retired ({}); 'loom stage reset {stage_id} \
             --kill-session' takes it down",
            inline_safe(detail)
        ));
    }
    let locked = lock_stage_disputes(work_dir, stage_id)?;
    escalate_to_human_review(stage_id, &locked.work_dir, |_| reason);
    let status = load_stage(stage_id, &locked.work_dir)?.status;
    if status != StageStatus::NeedsHumanReview {
        bail!("stage '{stage_id}' is {status}; its refused freezes could not be escalated");
    }
    Ok(())
}

/// The refusal's problems as a dispute keeps them: one line each, empty ones
/// dropped, flattened as untrusted text (a problem the writer reported is its
/// own text), bounded in number and size.
fn normalized_problems(problems: &[String]) -> Vec<String> {
    problems
        .iter()
        .flat_map(|problem| problem.split('\n'))
        .map(inline_safe)
        .filter(|problem| !problem.is_empty())
        .take(MAX_REFUSAL_PROBLEMS)
        .map(|problem| truncate_to_byte_limit(&problem, MAX_PROBLEM_BYTES))
        .collect()
}

/// The request receipt for `request_key`: a receipt key must be a plain id,
/// and a refusal's key carries a timestamp, so it is named by its hash.
fn receipt_key(request_key: &str) -> String {
    let digest = crate::relay::sha256_hex(request_key.as_bytes());
    format!("freeze-refusal-{}", &digest[..32])
}

/// Today's filing body, for every kind the transports admit and for loom's
/// own freeze refusal.
fn file_dispute_inner(
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
        return Ok(exhausted_response(&locked.work_dir, &stage, &kind));
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

/// The refusal for a spent budget. A freeze refusal only refuses: escalating
/// here would leave the contract writer alive, so the caller retires it, then
/// calls `escalate_freeze_refusal`.
fn exhausted_response(work_dir: &Path, stage: &Stage, kind: &DisputeKind) -> Response {
    if matches!(kind, DisputeKind::FreezeRefusal { .. }) {
        return refused(format!(
            "the freeze-refusal budget of stage '{}' is spent; escalate it instead",
            stage.id
        ));
    }
    escalate_exhausted(work_dir, stage, kind)
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
    check_filer(work_dir, stage, &kind)?;
    match kind {
        DisputeKind::Criterion { .. } => {
            bail!("a criterion is disputed with `loom stage dispute-criteria`")
        }
        DisputeKind::FreezeRefusal { problems } => {
            check_unfrozen(work_dir, stage)?;
            Ok(DisputeKind::FreezeRefusal { problems })
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

/// Refuse a kind the stage's current session may not file. The session record
/// `stage.session` names is the filer: the socket admits a dispute only from
/// the stage's own Stage session, the relay never from a contract writer, and
/// loom files a freeze refusal for the writer it refused. A stage with no
/// current session, or whose session record is gone, admits every kind but a
/// freeze refusal: that is an operator's filing.
fn check_filer(work_dir: &Path, stage: &Stage, kind: &DisputeKind) -> Result<()> {
    let session = match stage.session.as_deref() {
        Some(session_id) => load_session_exact(work_dir, session_id)?,
        None => None,
    };
    match session {
        Some(session) if !kind.filed_by(session.session_type) => bail!(
            "a {} dispute is not filed for stage '{}' while its {} session '{}' owns it",
            kind.name(),
            stage.id,
            session.session_type,
            session.id
        ),
        Some(_) => Ok(()),
        None if matches!(kind, DisputeKind::FreezeRefusal { .. }) => bail!(
            "stage '{}' has no contract writer whose freeze could be refused",
            stage.id
        ),
        None => Ok(()),
    }
}

/// A freeze refusal stands only while the contract phase is open: a standard
/// stage with contracts, executing, its contracts not frozen yet.
fn check_unfrozen(work_dir: &Path, stage: &Stage) -> Result<()> {
    if stage.stage_type != StageType::Standard || stage.contracts.is_empty() {
        bail!("stage '{}' has no contracts to freeze", stage.id);
    }
    if stage.status != StageStatus::Executing {
        bail!(
            "stage '{}' is {}, not executing its contract phase",
            stage.id,
            stage.status
        );
    }
    if load_freeze(work_dir, &stage.id)?.is_some() {
        bail!("the contracts of stage '{}' are already frozen", stage.id);
    }
    Ok(())
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

#[cfg(test)]
#[path = "../daemon/server/dispute_freeze_refusal_tests.rs"]
mod freeze_refusal_tests;
