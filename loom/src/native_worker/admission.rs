//! Daemon-side admission of lead-issued worker commands (common.md section 13).
//!
//! `admit_start` runs on the poll thread with the stage admission lock held:
//! it reads files and writes one record directory, and never spawns, sleeps or
//! touches the network. Nothing in a request is trusted: ids, model, effort and
//! the brief are validated here, and the brief's containment is decided on the
//! opened file (`brief::read_brief`).

use std::path::{Path, PathBuf};

use chrono::Utc;

use super::ids::{
    mint_invocation_id, validate_invocation_id, validate_message_id, validate_unit_id, IdError,
};
use super::index::{self, NativeDisposition};
use super::model::{
    CancelRequest, ControlMessage, Delivery, NativeManifest, NativeUnitState, Ownership, Phase,
    MAX_ACTIVE_PER_STAGE, MAX_MESSAGE_BYTES, NATIVE_DEADLINE_SECS,
};
use super::store::{self, MessageWrite, StoreError};
use crate::codex::{CODEX_EFFORTS, CODEX_MODELS};
use crate::models::stage::Stage;
use crate::utils::sha256_hex;

#[path = "admission_brief.rs"]
mod brief;
#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;

/// Longest cancel reason accepted, in bytes, after trimming.
const MAX_CANCEL_REASON_BYTES: usize = 1024;

/// A `start` command as the lead sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartRequest {
    pub unit_id: String,
    pub brief_path: PathBuf,
    pub brief_sha256: String,
    pub model: String,
    pub effort: String,
}

/// A `message` command as the lead sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRequest {
    pub unit_id: String,
    pub invocation_id: String,
    pub message_id: String,
    pub delivery: Delivery,
    pub text: String,
}

/// A `cancel` command; `invocation_id` is `None` for an operator cancel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelRequestIn {
    pub unit_id: String,
    pub invocation_id: Option<String>,
    pub reason: String,
}

/// What a successful `start` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionOutcome {
    Admitted { invocation_id: String },
    AlreadyAdmitted { invocation_id: String },
}

/// Admits `payload` for `stage`. Called with the stage admission lock held.
/// The checks run in the order of common.md section 13; the first failure is
/// the `Err` text and no later check runs.
pub fn admit_start(
    work_dir: &Path,
    stage: &Stage,
    owner_session_id: &str,
    payload: &StartRequest,
) -> Result<AdmissionOutcome, String> {
    check_request(&stage.id, payload)?;
    check_lane(stage)?;
    let (workspace_root, brief_bytes) = brief::read_brief(work_dir, stage, payload)?;
    check_not_unverified(work_dir, &stage.id)?;
    if let Some(invocation_id) = check_running_unit(work_dir, &stage.id, payload)? {
        return Ok(AdmissionOutcome::AlreadyAdmitted { invocation_id });
    }
    check_capacity(work_dir, &stage.id)?;
    let manifest = NativeManifest {
        v: 1,
        stage_id: stage.id.clone(),
        unit_id: payload.unit_id.clone(),
        invocation_id: mint_invocation_id()
            .map_err(|error| format!("cannot mint an invocation id: {error}"))?,
        owner_session_id: owner_session_id.to_string(),
        workspace_root,
        brief_path: payload.brief_path.clone(),
        brief_sha256: payload.brief_sha256.clone(),
        model: payload.model.clone(),
        effort: payload.effort.clone(),
        deadline_secs: NATIVE_DEADLINE_SECS,
        created_at: Utc::now(),
    };
    create_record(work_dir, &manifest, &brief_bytes)?;
    Ok(AdmissionOutcome::Admitted {
        invocation_id: manifest.invocation_id,
    })
}

/// Check 1: the ids, the model and the effort.
fn check_request(stage_id: &str, payload: &StartRequest) -> Result<(), String> {
    check_stage_id(stage_id)?;
    check_id("unit", &payload.unit_id, validate_unit_id)?;
    if !CODEX_MODELS.contains(&payload.model.as_str()) {
        return Err(format!(
            "model '{}' is not one of {}",
            payload.model,
            CODEX_MODELS.join(", ")
        ));
    }
    if !CODEX_EFFORTS.contains(&payload.effort.as_str()) {
        return Err(format!(
            "effort '{}' is not one of {}",
            payload.effort,
            CODEX_EFFORTS.join(", ")
        ));
    }
    Ok(())
}

/// Check 2: the stage licenses the codex lane.
fn check_lane(stage: &Stage) -> Result<(), String> {
    if stage.implementers.includes_codex() {
        Ok(())
    } else {
        Err(format!(
            "stage '{}' does not license the codex lane",
            stage.id
        ))
    }
}

/// Check 4: no record of the stage is corrupt or holds unproven ownership.
fn check_not_unverified(work_dir: &Path, stage_id: &str) -> Result<(), String> {
    let disposition = index::stage_disposition(work_dir, stage_id);
    match disposition {
        NativeDisposition::Unverified(_) => Err(disposition
            .blocker(stage_id)
            .unwrap_or_else(|| format!("stage '{stage_id}' has an unverified native worker"))),
        NativeDisposition::None | NativeDisposition::Active(_) => Ok(()),
    }
}

/// Check 5: a unit still running (or holding its boundary) keeps its
/// invocation. `Some(id)` when the identical request was already admitted.
fn check_running_unit(
    work_dir: &Path,
    stage_id: &str,
    payload: &StartRequest,
) -> Result<Option<String>, String> {
    let newest = index::unit_state(work_dir, stage_id, &payload.unit_id)
        .map_err(|error| unreadable(stage_id, &error))?;
    let Some(state) = newest else {
        return Ok(None);
    };
    if state.phase == Phase::Terminal && state.ownership != Ownership::Held {
        return Ok(None);
    }
    let manifest = &state.manifest;
    if manifest.brief_sha256 == payload.brief_sha256
        && manifest.model == payload.model
        && manifest.effort == payload.effort
    {
        return Ok(Some(manifest.invocation_id.clone()));
    }
    Err(format!(
        "unit {} is already running as {} with a different request",
        payload.unit_id, manifest.invocation_id
    ))
}

/// Check 6: the stage has room for one more non-terminal worker. A corrupt
/// record was already refused by check 4.
fn check_capacity(work_dir: &Path, stage_id: &str) -> Result<(), String> {
    let active = index::stage_units(work_dir, stage_id)
        .iter()
        .filter(|unit| matches!(unit, Ok(state) if state.phase != Phase::Terminal))
        .count();
    if active >= MAX_ACTIVE_PER_STAGE {
        return Err(format!(
            "stage '{stage_id}' already has {MAX_ACTIVE_PER_STAGE} running native Codex workers; wait for one to finish or cancel one"
        ));
    }
    Ok(())
}

/// Step 7: the record directory with its manifest, brief copy and `Admitted`.
fn create_record(work_dir: &Path, manifest: &NativeManifest, brief: &[u8]) -> Result<(), String> {
    store::create_invocation(work_dir, manifest, brief)
        .map(|_| ())
        .map_err(|error| unreadable(&manifest.stage_id, &error))
}

/// Stores `request` for the unit's running invocation; `Ok` is the ledger note.
pub fn accept_message(
    work_dir: &Path,
    stage_id: &str,
    request: &MessageRequest,
) -> Result<String, String> {
    check_stage_id(stage_id)?;
    check_id("unit", &request.unit_id, validate_unit_id)?;
    check_id("invocation", &request.invocation_id, validate_invocation_id)?;
    check_id("message", &request.message_id, validate_message_id)?;
    if request.text.is_empty() {
        return Err("the message text is empty".to_string());
    }
    if request.text.len() > MAX_MESSAGE_BYTES {
        return Err(format!(
            "the message text is longer than {MAX_MESSAGE_BYTES} bytes"
        ));
    }
    let state = running_invocation(work_dir, stage_id, &request.unit_id, &request.invocation_id)?;
    let dir = store::invocation_dir(work_dir, stage_id, &state.manifest.invocation_id);
    let message = ControlMessage {
        message_id: request.message_id.clone(),
        delivery: request.delivery,
        text: request.text.clone(),
        sha256: sha256_hex(request.text.as_bytes()),
        accepted_at: Utc::now(),
    };
    match store::write_message(&dir, &message).map_err(|error| unreadable(stage_id, &error))? {
        MessageWrite::Stored | MessageWrite::AlreadyStored => Ok(format!(
            "message {} accepted for {}",
            request.message_id, request.invocation_id
        )),
        MessageWrite::Conflict => Err(format!(
            "message id '{}' was already used for different text",
            request.message_id
        )),
    }
}

/// Requests cancellation of the unit's running invocation (the newest one
/// when `invocation_id` is `None`); `Ok` is the ledger note. A second cancel
/// keeps the first request and is `Ok`.
pub fn accept_cancel(
    work_dir: &Path,
    stage_id: &str,
    request: &CancelRequestIn,
    by: &str,
) -> Result<String, String> {
    check_stage_id(stage_id)?;
    check_id("unit", &request.unit_id, validate_unit_id)?;
    let reason = request.reason.trim();
    if reason.is_empty() {
        return Err("the cancel reason is empty".to_string());
    }
    if reason.len() > MAX_CANCEL_REASON_BYTES {
        return Err(format!(
            "the cancel reason is longer than {MAX_CANCEL_REASON_BYTES} bytes"
        ));
    }
    let state = match &request.invocation_id {
        Some(invocation_id) => {
            check_id("invocation", invocation_id, validate_invocation_id)?;
            running_invocation(work_dir, stage_id, &request.unit_id, invocation_id)?
        }
        None => newest_running(work_dir, stage_id, &request.unit_id)?,
    };
    let invocation_id = &state.manifest.invocation_id;
    let dir = store::invocation_dir(work_dir, stage_id, invocation_id);
    let cancel = CancelRequest {
        reason: reason.to_string(),
        requested_at: Utc::now(),
        by: by.to_string(),
    };
    store::write_cancel(&dir, &cancel).map_err(|error| unreadable(stage_id, &error))?;
    Ok(format!("cancel requested for {invocation_id}"))
}

/// The unit's newest invocation when it is `invocation_id` and not terminal.
fn running_invocation(
    work_dir: &Path,
    stage_id: &str,
    unit_id: &str,
    invocation_id: &str,
) -> Result<NativeUnitState, String> {
    let newest = index::unit_state(work_dir, stage_id, unit_id)
        .map_err(|error| unreadable(stage_id, &error))?;
    match newest {
        Some(state)
            if state.manifest.invocation_id == invocation_id && state.phase != Phase::Terminal =>
        {
            Ok(state)
        }
        _ => Err(format!(
            "{invocation_id} is not the running invocation of {unit_id}"
        )),
    }
}

/// The unit's newest invocation when it is not terminal.
fn newest_running(
    work_dir: &Path,
    stage_id: &str,
    unit_id: &str,
) -> Result<NativeUnitState, String> {
    let newest = index::unit_state(work_dir, stage_id, unit_id)
        .map_err(|error| unreadable(stage_id, &error))?;
    match newest {
        Some(state) if state.phase != Phase::Terminal => Ok(state),
        _ => Err(format!("unit {unit_id} has no running native worker")),
    }
}

/// The stage id names a directory under the record root, so it must have the
/// shape `create_invocation` and `index::stages` accept.
fn check_stage_id(stage_id: &str) -> Result<(), String> {
    check_id("stage", stage_id, validate_unit_id)
}

fn check_id(
    what: &str,
    value: &str,
    validate: fn(&str) -> Result<(), IdError>,
) -> Result<(), String> {
    validate(value).map_err(|error| format!("{what} id '{value}' is invalid ({error:?})"))
}

fn unreadable(stage_id: &str, error: &StoreError) -> String {
    format!("native worker records of stage '{stage_id}' could not be read: {error}")
}
