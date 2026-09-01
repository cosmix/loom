//! What a stage agent may ask the daemon to do about its OWN stage.
//!
//! A sandboxed stage agent is denied two things by design: reading
//! `.loom/work/user.token`, because that one credential authorizes every User RPC,
//! and writing `.loom/work/stages/<id>.md`, because stage state belongs to the
//! daemon. Between them they used to leave an agent that had finished its work
//! — or found it could not finish — with no way to say so. Peer identity
//! (`peer_identity.rs`) reopened that door for `CompleteStage`; this module is
//! where the policy for widening it to blocking and disputing lives, so that
//! widening it further is a deliberate edit in one place.
//!
//! Two separate questions, kept apart because neither answer implies the other:
//!
//! * *May the connection itself authorize this request?* —
//!   [`self_service_session`]. Everything outside the listed variants answers
//!   `None` and is refused without a valid token, so a User request added later
//!   is refused by default rather than silently inheriting the peer-identity
//!   path.
//! * *Does the named session actually own the named stage?* —
//!   [`session_owns_stage`], read from `.loom/work/`. Peer identity proves the
//!   caller IS session A; only this proves stage X is A's to act on. Without
//!   it, a live agent could reach across into another stage.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::daemon::protocol::Request;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::load_stage;

/// Upper bound on a session record, matching the other readers of the same
/// files (`peer_identity`, `control_complete`).
const MAX_SESSION_FILE_BYTES: usize = 1024 * 1024;

/// The session a request claims to be running inside, for the three RPCs a
/// stage agent is entitled to make about its OWN stage.
///
/// Every other request returns `None` and is refused without a valid token.
/// The `_` arm is what makes that the default: a variant added later has to be
/// listed here on purpose before the connection can ever authorize it.
///
/// An empty session id is returned as-is rather than as `None`; the caller
/// treats it as a claim that cannot be proven, which is the right outcome for
/// a request carrying neither a token nor a session.
pub(super) fn self_service_session(request: &Request) -> Option<&str> {
    match request {
        Request::CompleteStage { session_id, .. }
        | Request::DisputeCriteria { session_id, .. }
        | Request::BlockStage { session_id, .. } => Some(session_id),
        _ => None,
    }
}

/// The stage/session pair whose ownership must be proven before the handler
/// runs, or `None` when there is nothing to prove.
///
/// `CompleteStage` is deliberately absent: `control_complete` re-validates the
/// identical binding under the sessions-directory lock, together with the
/// `Executing` requirement that only completion imposes. Checking it here too
/// would report that failure as an authentication error and lose the handler's
/// more precise message.
///
/// A request with an empty session id — an operator shell that authenticated
/// with the user token — has no session whose ownership could be checked, and
/// the token is what carries it.
pub(super) fn ownership_to_enforce(request: &Request) -> Option<(&str, &str)> {
    match request {
        Request::DisputeCriteria {
            stage_id,
            session_id,
            ..
        }
        | Request::BlockStage {
            stage_id,
            session_id,
            ..
        } if !session_id.is_empty() => Some((stage_id, session_id)),
        _ => None,
    }
}

/// Whether `session_id` is the session currently assigned to `stage_id`.
///
/// Both directions are checked — the stage's `session` field and the session
/// record's `stage_id` — plus [`SessionType::Stage`] and
/// [`SessionStatus::Running`], so a live session cannot act on a stage that is
/// not its own and a finished session cannot act at all.
///
/// Deliberately NOT checked here: [`crate::models::stage::StageStatus`].
/// Completion needs the stage to still be `Executing` and keeps that
/// requirement in `control_complete`; a stage may legitimately be blocked or
/// disputed from other states, and each handler validates its own transition.
///
/// This runs before the handler takes any lock, so the binding it proves could
/// in principle change before the handler mutates. That is acceptable for an
/// authorization pre-check whose handlers re-read under their own locks: the
/// window is between two facts about the same session, not a way to smuggle a
/// different one through.
pub(super) fn session_owns_stage(work_dir: &Path, stage_id: &str, session_id: &str) -> Result<()> {
    // Both ids arrive unvalidated from the wire and both are turned into
    // paths below, so traversal shapes have to die before any file is touched.
    crate::validation::validate_id(stage_id).context("invalid stage id")?;
    crate::validation::validate_id(session_id).context("invalid session id")?;

    let stage = load_stage(stage_id, work_dir)?;
    if stage.session.as_deref() != Some(session_id) {
        bail!("session '{session_id}' is not active for stage '{stage_id}'");
    }

    let relative = PathBuf::from("sessions").join(format!("{session_id}.md"));
    let content =
        crate::fs::safe_read::read_to_string_bounded(work_dir, &relative, MAX_SESSION_FILE_BYTES)
            .with_context(|| format!("failed to read active session '{session_id}'"))?;
    let session: Session =
        parse_from_markdown(&content, "session").context("invalid active session file")?;
    if session.id != session_id
        || session.stage_id.as_deref() != Some(stage_id)
        || session.session_type != SessionType::Stage
        || session.status != SessionStatus::Running
    {
        bail!("request does not match the active running stage session");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::session_files::save_session;
    use crate::models::stage::{Stage, StageStatus};
    use crate::verify::transitions::save_stage;
    use tempfile::TempDir;

    fn active_pair(work_dir: &Path, stage_id: &str) -> Session {
        let mut session = Session::new();
        session.assign_to_stage(stage_id.to_string());
        session.status = SessionStatus::Running;
        let mut stage = Stage::new(stage_id.to_string(), None);
        stage.id = stage_id.to_string();
        stage.status = StageStatus::Executing;
        stage.session = Some(session.id.clone());
        save_stage(&stage, work_dir).unwrap();
        save_session(&session, work_dir).unwrap();
        session
    }

    fn block(session_id: &str) -> Request {
        Request::BlockStage {
            auth_token: "t".to_string(),
            stage_id: "build-api".to_string(),
            session_id: session_id.to_string(),
            reason: "r".to_string(),
        }
    }

    #[test]
    fn only_the_three_own_stage_requests_can_be_authorized_by_the_connection() {
        assert_eq!(Some("s1"), self_service_session(&block("s1")));
        assert_eq!(
            Some("s2"),
            self_service_session(&Request::CompleteStage {
                auth_token: "t".to_string(),
                stage_id: "build-api".to_string(),
                session_id: "s2".to_string(),
                nonce: "0123456789abcdef0123456789abcdef".to_string(),
            })
        );
        assert_eq!(
            Some("s3"),
            self_service_session(&Request::DisputeCriteria {
                auth_token: "t".to_string(),
                stage_id: "build-api".to_string(),
                session_id: "s3".to_string(),
                criterion_index: 0,
                reason: "r".to_string(),
                evidence_commit: None,
                failure_output: None,
            })
        );

        // The default. A User RPC that is not about the caller's own stage has
        // no session to name and must stay behind the token.
        assert_eq!(
            None,
            self_service_session(&Request::Ping {
                auth_token: "t".to_string()
            })
        );
        assert_eq!(
            None,
            self_service_session(&Request::SubscribeLogs {
                auth_token: "t".to_string()
            })
        );
    }

    #[test]
    fn completion_ownership_is_left_to_its_own_locked_handler() {
        assert_eq!(
            None,
            ownership_to_enforce(&Request::CompleteStage {
                auth_token: "t".to_string(),
                stage_id: "build-api".to_string(),
                session_id: "s1".to_string(),
                nonce: "0123456789abcdef0123456789abcdef".to_string(),
            })
        );
        assert_eq!(
            Some(("build-api", "s1")),
            ownership_to_enforce(&block("s1"))
        );
        // Nothing to prove when no session is named: the token carried it.
        assert_eq!(None, ownership_to_enforce(&block("")));
    }

    #[test]
    fn a_live_session_owns_only_its_own_stage() {
        let temp = TempDir::new().unwrap();
        let mine = active_pair(temp.path(), "build-api");
        let theirs = active_pair(temp.path(), "other-stage");

        assert!(session_owns_stage(temp.path(), "build-api", &mine.id).is_ok());
        // The escalation this check exists to stop: a genuinely live session
        // naming somebody else's stage.
        assert!(session_owns_stage(temp.path(), "other-stage", &mine.id).is_err());
        assert!(session_owns_stage(temp.path(), "build-api", &theirs.id).is_err());
    }

    #[test]
    fn a_session_that_is_no_longer_running_owns_nothing() {
        let temp = TempDir::new().unwrap();
        let mut session = active_pair(temp.path(), "build-api");
        session.status = SessionStatus::Completed;
        save_session(&session, temp.path()).unwrap();

        let error = session_owns_stage(temp.path(), "build-api", &session.id)
            .unwrap_err()
            .to_string();

        assert!(error.contains("active running stage session"), "{error}");
    }

    #[test]
    fn ids_shaped_like_paths_are_refused_before_any_file_is_touched() {
        let temp = TempDir::new().unwrap();
        active_pair(temp.path(), "build-api");

        assert!(session_owns_stage(temp.path(), "../../etc/passwd", "s1").is_err());
        assert!(session_owns_stage(temp.path(), "build-api", "../../etc/passwd").is_err());
    }

    #[test]
    fn an_unknown_session_owns_nothing() {
        let temp = TempDir::new().unwrap();
        active_pair(temp.path(), "build-api");

        assert!(session_owns_stage(temp.path(), "build-api", "session-nope").is_err());
    }
}
