//! Narrow daemon-side stage completion transition.

use crate::daemon::protocol::Response;
use crate::fs::locking::locked_dir_update;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, update_stage};
use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const NONCE_LEN: usize = 32;

pub(super) fn handle_complete_stage(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    nonce: &str,
) -> Result<Response> {
    validate_request_fields(stage_id, session_id, nonce)?;
    if replay_path(work_dir, nonce).exists() {
        bail!("completion request nonce was already consumed");
    }

    let sessions_dir = work_dir.join("sessions");
    if !sessions_dir.is_dir() {
        bail!("completion sessions directory is unavailable");
    }
    locked_dir_update(&sessions_dir, || {
        // Hold the session-directory lock from the Running check through the
        // stage transition. Canonical session writers take this same lock, so
        // a crash/completion update cannot invalidate the authorization fact
        // between validation and mutation.
        validate_active_identity(work_dir, stage_id, session_id)?;
        update_stage(stage_id, work_dir, |stage| {
            if stage.status != StageStatus::Executing {
                bail!("stage is no longer executing");
            }
            if stage.session.as_deref() != Some(session_id) {
                bail!("stage session changed before completion was applied");
            }
            stage.try_complete(None)
        })?;
        consume_nonce(work_dir, nonce)?;
        Ok(())
    })?;
    Ok(Response::Ok)
}

fn validate_request_fields(stage_id: &str, session_id: &str, nonce: &str) -> Result<()> {
    crate::validation::validate_id(stage_id).context("invalid completion stage id")?;
    crate::validation::validate_id(session_id).context("invalid completion session id")?;
    if nonce.len() != NONCE_LEN || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("completion nonce must be exactly 32 hexadecimal characters");
    }
    Ok(())
}

/// The stage/session binding completion requires, checked under the caller's
/// sessions-directory lock.
///
/// The ownership half is shared with block and dispute
/// (`self_service::session_owns_stage`), so tightening the rule tightens it for
/// all three. The `Executing` requirement stays here because it is completion's
/// alone: a stage may legitimately be blocked or disputed from other states.
fn validate_active_identity(work_dir: &Path, stage_id: &str, session_id: &str) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)?;
    if stage.status != StageStatus::Executing {
        bail!("stage '{stage_id}' is not executing");
    }
    super::super::self_service::session_owns_stage(work_dir, stage_id, session_id)
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
mod tests {
    use super::*;
    use crate::fs::session_files::save_session;
    use crate::models::session::{Session, SessionStatus};
    use crate::models::stage::Stage;
    use crate::verify::transitions::save_stage;
    use tempfile::TempDir;

    fn active_pair(work_dir: &Path, stage_id: &str) -> (Stage, Session) {
        let mut session = Session::new();
        session.stage_id = Some(stage_id.to_string());
        session.status = SessionStatus::Running;
        let mut stage = Stage::new(stage_id.to_string(), None);
        stage.id = stage_id.to_string();
        stage.status = StageStatus::Executing;
        stage.session = Some(session.id.clone());
        save_stage(&stage, work_dir).unwrap();
        save_session(&session, work_dir).unwrap();
        (stage, session)
    }

    fn stage_snapshot(work_dir: &Path, stage_id: &str) -> String {
        serde_json::to_string(&load_stage(stage_id, work_dir).unwrap()).unwrap()
    }

    #[test]
    fn accepts_one_exact_active_completion_and_rejects_replay() {
        let temp = TempDir::new().unwrap();
        let (_, session) = active_pair(temp.path(), "build-api");
        let nonce = "0123456789abcdef0123456789abcdef";

        assert!(matches!(
            handle_complete_stage(temp.path(), "build-api", &session.id, nonce).unwrap(),
            Response::Ok
        ));
        assert_eq!(
            load_stage("build-api", temp.path()).unwrap().status,
            StageStatus::Completed
        );
        assert!(
            handle_complete_stage(temp.path(), "build-api", &session.id, nonce)
                .unwrap_err()
                .to_string()
                .contains("already consumed")
        );
    }

    #[test]
    fn rejects_cross_stage_and_cross_session_without_mutation() {
        let temp = TempDir::new().unwrap();
        let (_, session) = active_pair(temp.path(), "build-api");
        let (_, other_session) = active_pair(temp.path(), "other-stage");
        let build_before = stage_snapshot(temp.path(), "build-api");
        let other_before = stage_snapshot(temp.path(), "other-stage");
        let cross_stage = handle_complete_stage(
            temp.path(),
            "other-stage",
            &session.id,
            "11111111111111111111111111111111",
        );
        let cross_session = handle_complete_stage(
            temp.path(),
            "build-api",
            "session-other",
            "22222222222222222222222222222222",
        );

        assert!(cross_stage.is_err());
        assert!(cross_session.is_err());
        assert_eq!(stage_snapshot(temp.path(), "build-api"), build_before);
        assert_eq!(stage_snapshot(temp.path(), "other-stage"), other_before);
        assert_eq!(other_session.status, SessionStatus::Running);
        assert!(!replay_path(temp.path(), "11111111111111111111111111111111").exists());
        assert!(!replay_path(temp.path(), "22222222222222222222222222222222").exists());
    }

    #[test]
    fn rejects_preconsumed_nonce_without_mutating_active_stage() {
        let temp = TempDir::new().unwrap();
        let (_, session) = active_pair(temp.path(), "build-api");
        let before = stage_snapshot(temp.path(), "build-api");
        let nonce = "33333333333333333333333333333333";
        consume_nonce(temp.path(), nonce).unwrap();

        let error = handle_complete_stage(temp.path(), "build-api", &session.id, nonce)
            .unwrap_err()
            .to_string();

        assert!(error.contains("already consumed"));
        assert_eq!(stage_snapshot(temp.path(), "build-api"), before);
    }

    #[test]
    fn rejects_non_running_session_without_mutating_stage_or_consuming_nonce() {
        let temp = TempDir::new().unwrap();
        let (_, mut session) = active_pair(temp.path(), "build-api");
        session.status = SessionStatus::Completed;
        save_session(&session, temp.path()).unwrap();
        let before = stage_snapshot(temp.path(), "build-api");
        let nonce = "44444444444444444444444444444444";

        let error = handle_complete_stage(temp.path(), "build-api", &session.id, nonce)
            .unwrap_err()
            .to_string();

        assert!(error.contains("active running stage session"));
        assert_eq!(stage_snapshot(temp.path(), "build-api"), before);
        assert!(!replay_path(temp.path(), nonce).exists());
    }

    #[test]
    fn completion_request_uses_user_capability_and_has_no_extensible_payload() {
        use crate::daemon::protocol::{Capability, Request};

        let request = Request::CompleteStage {
            auth_token: "secret".to_string(),
            stage_id: "build-api".to_string(),
            session_id: "session-123".to_string(),
            nonce: "0123456789abcdef0123456789abcdef".to_string(),
        };
        assert_eq!(request.required_capability(), Capability::User);
        let encoded = serde_json::to_string(&request).unwrap();
        for forbidden in [
            "command",
            "path",
            "no_verify",
            "force_unsafe",
            "assume_merged",
        ] {
            assert!(!encoded.contains(forbidden), "unexpected field: {encoded}");
        }
    }
}
