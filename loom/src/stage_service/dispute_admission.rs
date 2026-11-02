//! What admits a plan v2 dispute: who may file the kind, and the daemon's own
//! re-derivation of the evidence it names.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::fs::session_files::load_session_exact_unlocked;
use crate::models::dispute::{select_events, select_findings, select_impact, DisputeKind};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::contracts::store::load_freeze;
use crate::verify::impact_tests::{check_failure_id, load_observed};
use crate::verify::review::store::open_findings;

/// Admission decisions are durable; errors reading their evidence are not.
pub(super) enum Admission {
    Accepted(DisputeKind),
    Rejected(String),
}

/// Refuse a kind the stage's current session may not file. The session record
/// `stage.session` names is the filer: the socket admits a dispute only from
/// the stage's own Stage session, the relay never from a contract writer, and
/// loom files a freeze refusal for the writer it refused. A stage with no
/// current session, or whose session record is gone, admits every kind but a
/// freeze refusal: that is an operator's filing.
///
/// Runs under the exclusive stage lock, and `complete_stage` takes the
/// sessions-directory lock before the stage lock, so this read takes no lock.
pub(super) fn check_filer(
    work_dir: &Path,
    stage: &Stage,
    kind: &DisputeKind,
) -> Result<Option<String>> {
    let session = match stage.session.as_deref() {
        Some(session_id) => load_session_exact_unlocked(work_dir, session_id)?,
        None => None,
    };
    match session {
        Some(session) if !kind.filed_by(session.session_type) => Ok(Some(format!(
            "a {} dispute is not filed for stage '{}' while its {} session '{}' owns it",
            kind.name(),
            stage.id,
            session.session_type,
            session.id
        ))),
        Some(_) => Ok(None),
        None if matches!(kind, DisputeKind::FreezeRefusal { .. }) => Ok(Some(format!(
            "stage '{}' has no contract writer whose freeze could be refused",
            stage.id
        ))),
        None => Ok(None),
    }
}

/// A freeze refusal stands only while the contract phase is open: a standard
/// stage with contracts, executing, its contracts not frozen yet.
fn check_unfrozen(work_dir: &Path, stage: &Stage) -> Result<Option<String>> {
    if stage.stage_type != StageType::Standard || stage.contracts.is_empty() {
        return Ok(Some(format!(
            "stage '{}' has no contracts to freeze",
            stage.id
        )));
    }
    if stage.status != StageStatus::Executing {
        return Ok(Some(format!(
            "stage '{}' is {}, not executing its contract phase",
            stage.id, stage.status
        )));
    }
    if load_freeze(work_dir, &stage.id)?.is_some() {
        return Ok(Some(format!(
            "the contracts of stage '{}' are already frozen",
            stage.id
        )));
    }
    Ok(None)
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

/// The dispute to record: `kind` with every id checked against what exists
/// now and its evidence re-derived by the daemon.
pub(super) fn confirm(work_dir: &Path, stage: &Stage, kind: DisputeKind) -> Result<Admission> {
    if stage.plan_version != 2 {
        return Ok(Admission::Rejected(format!(
            "stage '{}' is not a plan version 2 stage; only its acceptance criteria, wiring \
             checks and wiring tests can be disputed, with `loom stage dispute-criteria --field`",
            stage.id
        )));
    }
    if let Some(reason) = check_filer(work_dir, stage, &kind)? {
        return Ok(Admission::Rejected(reason));
    }
    if let DisputeKind::FreezeRefusal { problems } = kind {
        return Ok(match check_unfrozen(work_dir, stage)? {
            Some(reason) => Admission::Rejected(reason),
            None => Admission::Accepted(DisputeKind::FreezeRefusal { problems }),
        });
    }
    Ok(match confirm_evidence(work_dir, stage, kind) {
        Ok(kind) => Admission::Accepted(kind),
        Err(error) => Admission::Rejected(format!("{error:#}")),
    })
}

fn confirm_evidence(work_dir: &Path, stage: &Stage, kind: DisputeKind) -> Result<DisputeKind> {
    match kind {
        DisputeKind::Criterion { .. } => {
            bail!("a criterion is disputed with `loom stage dispute-criteria`")
        }
        DisputeKind::FreezeRefusal { .. } => unreachable!("freeze refusals are confirmed above"),
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
            let events = crate::stage_service::observer::integrity_events(work_dir, stage)?;
            let evidence = select_events(&events, &event_ids)?;
            Ok(DisputeKind::Integrity {
                event_ids,
                evidence,
            })
        }
        // Whatever evidence the client sent is dropped: the observed record is
        // it, the host or operator completion's latest observed failure.
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
