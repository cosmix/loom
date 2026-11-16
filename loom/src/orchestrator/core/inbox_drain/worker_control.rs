//! Apply lead-issued native worker commands while holding stage admission.

use std::path::Path;

use crate::codex::{configured_backend, CodexBackend};
use crate::fs::stage_admission::lock_for_daemon;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::native_worker::admission::{
    accept_cancel, accept_message, admit_start, AdmissionOutcome, CancelRequestIn, MessageRequest,
    StartRequest,
};
use crate::relay::WorkerControlPayload;

use super::apply::require_owner;
use super::{InboxHost, Settle};

pub(super) fn apply_worker_control(
    host: &dyn InboxHost,
    stage_id: &str,
    record: &Session,
    payload: WorkerControlPayload,
) -> Settle {
    apply_with_backend(host, stage_id, record, payload, configured_backend)
}

/// `backend` is injected so tests need no config files. Only the configured
/// backend is read here: qualification (bubblewrap probe, up to 10 s) must not
/// run on the poll thread under the admission lock, and the supervisor's
/// preflight (`native_worker::supervisor::preflight`) settles an unqualified
/// host as a failed worker before anything is spawned.
pub(super) fn apply_with_backend(
    host: &dyn InboxHost,
    stage_id: &str,
    record: &Session,
    payload: WorkerControlPayload,
    backend: fn(&Path) -> CodexBackend,
) -> Settle {
    let work_dir = host.work_dir();
    let stage = match require_owner(work_dir, stage_id, record) {
        Ok(stage) => stage,
        Err(reason) => return Settle::Refused(reason),
    };
    let _lock = match lock_for_daemon(work_dir, stage_id) {
        Ok(fd) => fd,
        Err(_) => {
            return Settle::Refused(format!(
                "another loom command is changing stage '{stage_id}'; repeat the command in a moment"
            ));
        }
    };
    settle(match payload {
        WorkerControlPayload::Start {
            unit_id,
            brief_path,
            brief_sha256,
            model,
            effort,
        } => apply_start(
            work_dir,
            &stage,
            record,
            &StartRequest {
                unit_id,
                brief_path,
                brief_sha256,
                model,
                effort,
            },
            backend,
        ),
        other => apply_running(work_dir, stage_id, record, other),
    })
}

fn settle(result: Result<String, String>) -> Settle {
    match result {
        Ok(note) => Settle::Applied(Some(note)),
        Err(reason) => Settle::Refused(reason),
    }
}

fn apply_start(
    work_dir: &Path,
    stage: &Stage,
    record: &Session,
    request: &StartRequest,
    backend: fn(&Path) -> CodexBackend,
) -> Result<String, String> {
    if backend(work_dir) != CodexBackend::Native {
        return Err(
            "codex.backend is legacy; native Codex workers need codex.backend = native".into(),
        );
    }
    let unit = &request.unit_id;
    match admit_start(work_dir, stage, &record.id, request)? {
        AdmissionOutcome::Admitted { invocation_id } => {
            Ok(format!("native worker {unit} admitted as {invocation_id}"))
        }
        AdmissionOutcome::AlreadyAdmitted { invocation_id } => Ok(format!(
            "native worker {unit} already admitted as {invocation_id}"
        )),
    }
}

fn apply_running(
    work_dir: &Path,
    stage_id: &str,
    record: &Session,
    payload: WorkerControlPayload,
) -> Result<String, String> {
    match payload {
        WorkerControlPayload::Message {
            unit_id,
            invocation_id,
            message_id,
            delivery,
            text,
        } => accept_message(
            work_dir,
            stage_id,
            &MessageRequest {
                unit_id,
                invocation_id,
                message_id,
                delivery,
                text,
            },
        ),
        WorkerControlPayload::Cancel {
            unit_id,
            invocation_id,
            reason,
        } => accept_cancel(
            work_dir,
            stage_id,
            &CancelRequestIn {
                unit_id,
                invocation_id: Some(invocation_id),
                reason,
            },
            &format!("lead:{}", record.id),
        ),
        WorkerControlPayload::Start { .. } => unreachable!("starts are handled by apply_start"),
    }
}
