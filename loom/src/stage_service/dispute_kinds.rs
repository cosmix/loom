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

use anyhow::{bail, Result};
use std::path::Path;

#[path = "dispute_admission.rs"]
mod admission;

use super::dispute::transition_refusal;
use super::dispute_store::{
    escalate_to_human_review, file_under_stage_lock, lock_stage_disputes, Publication,
};
use super::Response;
use crate::context::untrusted::inline_safe;
use crate::models::dispute::{truncate_to_byte_limit, DisputeKind, DISPUTE_REASON_MAX_BYTES};
use crate::models::stage::dispute_budgets::{self, MAX_DISPUTES_PER_KIND};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::load_stage;
use admission::{confirm, Admission};

#[cfg(test)]
use {
    crate::models::dispute::select_findings, crate::models::stage::StageType,
    crate::verify::review::store::open_findings, crate::verify::transitions::update_stage,
    admission::check_filer,
};

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

#[cfg(test)]
pub(crate) fn handle_file_dispute(
    work_dir: &Path,
    stage_id: &str,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
) -> Result<Response> {
    handle_file_dispute_identified(work_dir, stage_id, kind, reason, evidence_commit, None)
}

#[cfg(test)]
pub(crate) fn handle_file_dispute_identified(
    work_dir: &Path,
    stage_id: &str,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
    request_id: Option<&str>,
) -> Result<Response> {
    handle_file_dispute_for_session(
        work_dir,
        stage_id,
        kind,
        reason,
        evidence_commit,
        (request_id, None),
    )
}

pub(crate) fn handle_file_dispute_for_session(
    work_dir: &Path,
    stage_id: &str,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
    identity: (Option<&str>, Option<&str>),
) -> Result<Response> {
    if matches!(kind, DisputeKind::FreezeRefusal { .. }) {
        return Ok(refused(LOOM_ONLY.to_string()));
    }
    file_dispute_inner(work_dir, stage_id, kind, reason, evidence_commit, identity)
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
    match file_dispute_inner(work_dir, stage_id, kind, reason, None, (Some(&key), None))? {
        Response::DisputeCreated { id } => Ok(FreezeRefusalFiling::Filed(format!(
            "freeze-refusal dispute {id} filed"
        ))),
        Response::Error { message } => Ok(FreezeRefusalFiling::Declined(message)),
        other => bail!("unexpected freeze-refusal filing outcome {other:?}"),
    }
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
    identity: (Option<&str>, Option<&str>),
) -> Result<Response> {
    // The stage id arrives unvalidated from the wire and names directories.
    if let Err(error) = crate::validation::validate_id(stage_id) {
        return Ok(refused(format!("invalid stage_id: {error}")));
    }
    let reason = truncate_to_byte_limit(&reason, DISPUTE_REASON_MAX_BYTES);
    let locked = lock_stage_disputes(work_dir, stage_id)?;
    #[cfg(test)]
    super::dispute_race_tests::before_stage_update();
    let (request_id, _) = identity;
    file_under_stage_lock(&locked, stage_id, request_id, |stage, publication| {
        let filing = Filing {
            work_dir: &locked.work_dir,
            kind,
            reason,
            evidence_commit,
            identity,
        };
        file_on_locked_stage(stage, publication, filing)
    })
}

/// What one filing asks for, past the stage id.
struct Filing<'a> {
    work_dir: &'a Path,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
    identity: (Option<&'a str>, Option<&'a str>),
}

fn file_on_locked_stage(
    stage: &mut Stage,
    publication: &mut Publication<'_>,
    filing: Filing<'_>,
) -> Result<Response> {
    let (request_id, session_id) = filing.identity;
    if session_id.is_some_and(|id| !super::ownership::matches_attempt(stage, id)) {
        return Ok(refused("request belongs to a stale stage session".into()));
    }
    if let Some(existing) = super::receipts::existing(stage, request_id)? {
        return Ok(existing);
    }
    let kind = match confirm(filing.work_dir, stage, filing.kind)? {
        Admission::Accepted(kind) => kind,
        Admission::Rejected(reason) => return Ok(refused(reason)),
    };
    if dispute_budgets::exhausted(stage, &kind) {
        return Ok(exhausted_response(stage, &kind));
    }
    let reason = filing.reason;
    if let Some(refusal) = transition_refusal(stage, &reason) {
        return Ok(refusal);
    }
    #[cfg(test)]
    super::dispute_race_tests::after_admission();
    let record = super::dispute::build_record(
        stage,
        kind.clone(),
        reason.clone(),
        filing.evidence_commit,
        None,
    );
    let id = publication.write(record)?;
    dispute_budgets::spend(stage, &kind);
    super::receipts::remember(stage, request_id, id);
    stage.tally.evidence_rounds = 0;
    stage.try_request_adjudication(Some(reason))?;
    #[cfg(test)]
    super::dispute_race_tests::after_publication();
    Ok(Response::DisputeCreated { id })
}

fn refused(message: String) -> Response {
    Response::Error { message }
}

/// The refusal for a spent budget. A freeze refusal only refuses: escalating
/// here would leave the contract writer alive, so the caller retires it, then
/// calls `escalate_freeze_refusal`.
fn exhausted_response(stage: &mut Stage, kind: &DisputeKind) -> Response {
    if matches!(kind, DisputeKind::FreezeRefusal { .. }) {
        return refused(format!(
            "the freeze-refusal budget of stage '{}' is spent; escalate it instead",
            stage.id
        ));
    }
    escalate_exhausted(stage, kind)
}

/// Escalate to NeedsHumanReview and refuse, as the criterion path does when
/// its budget is spent.
fn escalate_exhausted(stage: &mut Stage, kind: &DisputeKind) -> Response {
    let name = kind.name();
    let count = dispute_budgets::filed(stage, kind);
    let reason = format!(
        "Dispute budget exhausted ({count} of {MAX_DISPUTES_PER_KIND} {name} disputes filed)"
    );
    if let Err(error) = stage.try_request_human_review(reason) {
        tracing::warn!(stage = %stage.id, %error, "dispute budget exhausted but stage could not be escalated");
    }
    refused(format!(
        "Dispute budget exhausted ({} {name} disputes filed; max is {MAX_DISPUTES_PER_KIND}).",
        dispute_budgets::filed(stage, kind)
    ))
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
