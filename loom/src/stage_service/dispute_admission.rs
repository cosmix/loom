use anyhow::{bail, Result};
use std::path::Path;

use crate::fs::session_files::load_session_exact;
use crate::models::dispute::DisputeKind;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::contracts::store::load_freeze;

/// Refuse a kind the stage's current session may not file. The session record
/// `stage.session` names is the filer: the socket admits a dispute only from
/// the stage's own Stage session, the relay never from a contract writer, and
/// loom files a freeze refusal for the writer it refused. A stage with no
/// current session, or whose session record is gone, admits every kind but a
/// freeze refusal: that is an operator's filing.
pub(super) fn check_filer(
    work_dir: &Path,
    stage: &Stage,
    kind: &DisputeKind,
) -> Result<Option<String>> {
    let session = match stage.session.as_deref() {
        Some(session_id) => load_session_exact(work_dir, session_id)?,
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
pub(super) fn check_unfrozen(work_dir: &Path, stage: &Stage) -> Result<Option<String>> {
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

pub(super) fn check_frozen(work_dir: &Path, stage_id: &str, contract_id: &str) -> Result<()> {
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
