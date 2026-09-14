//! Narrow daemon-side stage completion transition.

use crate::daemon::protocol::Response;
use crate::fs::locking::locked_dir_update;
use crate::models::session::{SessionStatus, SessionType};
use crate::models::stage::{StageStatus, StageType};
use crate::verify::transitions::{load_stage, update_stage};
use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const NONCE_LEN: usize = 32;

/// The session kinds that complete their own stage through the broker: a
/// worktree stage session, and a knowledge session in the main repository.
const COMPLETING_SESSION_TYPES: &[SessionType] = &[SessionType::Stage, SessionType::Knowledge];

pub(super) fn handle_complete_stage(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    nonce: &str,
    evidence_nonce: &str,
) -> Result<Response> {
    validate_request_fields(stage_id, session_id, nonce, evidence_nonce)?;
    if replay_path(work_dir, nonce).exists() {
        bail!("completion request nonce was already consumed");
    }

    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.is_dir() {
        bail!("completion sessions directory is unavailable");
    }
    // Hold the session-directory lock from the Running check through the
    // stage transition. Canonical session writers take this same lock.
    locked_dir_update(&sessions_dir, || {
        complete_under_lock(work_dir, stage_id, session_id, nonce, evidence_nonce)
    })?;
    retire_completed_session(work_dir, session_id);
    Ok(Response::Ok)
}

fn complete_under_lock(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    nonce: &str,
    evidence_nonce: &str,
) -> Result<()> {
    validate_active_identity(work_dir, stage_id, session_id)?;
    let stage = load_stage(stage_id, work_dir)?;
    let verified_commit =
        super::completion_evidence::verified_commit(work_dir, &stage, session_id, evidence_nonce)?;
    let completed_stage = update_stage(stage_id, work_dir, |stage| {
        if stage.status != StageStatus::Executing {
            bail!("stage is no longer executing");
        }
        if stage.session.as_deref() != Some(session_id) {
            bail!("stage session changed before completion was applied");
        }
        if stage.stage_type == StageType::Knowledge {
            stage.merged = true;
        }
        stage.try_complete(None)
    })?;
    consume_nonce(work_dir, nonce)?;
    if let Err(error) = super::completion_evidence::persist_acceptance_receipt(
        work_dir,
        &completed_stage,
        session_id,
        evidence_nonce,
        nonce,
        verified_commit,
    ) {
        tracing::warn!(stage_id, session_id, error = %format!("{error:#}"), "failed to persist accepted completion receipt");
    }
    Ok(())
}

/// The daemon's half of `commands::stage::session::cleanup_session_resources`:
/// a sandboxed session cannot write its own record or signal file, so the
/// broker does it once the transition has landed (outside the sessions-dir
/// lock, which the record write takes itself). Best-effort, as in the CLI:
/// the stage is already complete.
fn retire_completed_session(work_dir: &Path, session_id: &str) {
    let completed = SessionStatus::Completed;
    if let Err(error) =
        crate::commands::stage::session::update_session_status(work_dir, session_id, completed)
    {
        tracing::warn!(session_id = %session_id, error = %format!("{error:#}"), "failed to mark the completed session's record Completed");
    }
    if let Err(error) = crate::orchestrator::signals::remove_signal(session_id, work_dir) {
        tracing::warn!(session_id = %session_id, error = %format!("{error:#}"), "failed to remove the completed session's signal file");
    }
}

fn validate_request_fields(
    stage_id: &str,
    session_id: &str,
    nonce: &str,
    evidence_nonce: &str,
) -> Result<()> {
    crate::validation::validate_id(stage_id).context("invalid completion stage id")?;
    crate::validation::validate_id(session_id).context("invalid completion session id")?;
    if nonce.len() != NONCE_LEN || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("completion nonce must be exactly 32 hexadecimal characters");
    }
    if evidence_nonce.len() != NONCE_LEN
        || !evidence_nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("evidence nonce must be exactly 32 hexadecimal characters");
    }
    if evidence_nonce == nonce {
        bail!("completion nonce and evidence nonce must be distinct");
    }
    Ok(())
}

/// The stage/session binding completion requires, checked under the caller's
/// sessions-directory lock.
///
/// The ownership half is shared with block and dispute
/// (`self_service::session_owns_stage_as`), so tightening the rule tightens it
/// for all three; completion alone also admits knowledge sessions. The
/// `Executing` requirement stays here because it is completion's alone: a
/// stage may legitimately be blocked or disputed from other states.
pub(crate) fn validate_active_identity(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)?;
    if stage.status != StageStatus::Executing {
        bail!("stage '{stage_id}' is not executing");
    }
    super::self_service::session_owns_stage_as(
        work_dir,
        stage_id,
        session_id,
        COMPLETING_SESSION_TYPES,
    )
}

fn replay_path(work_dir: &Path, nonce: &str) -> PathBuf {
    work_dir.join("control-completions").join(nonce)
}

fn consume_nonce(work_dir: &Path, nonce: &str) -> Result<()> {
    let dir = work_dir.join("control-completions");
    fs::create_dir_all(&dir).context("failed to create completion replay directory")?;
    let path = replay_path(work_dir, nonce);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow::anyhow!("completion request nonce was already consumed")
            } else {
                anyhow::anyhow!(error).context("failed to consume completion request nonce")
            }
        })?;
    file.write_all(b"consumed\n")
        .context("failed to persist completion replay marker")
}

#[cfg(test)]
#[path = "control_complete_tests.rs"]
mod tests;
