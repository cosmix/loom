//! What a stage agent may ask the daemon to do about its OWN stage.
//!
//! A sandboxed stage agent is denied two things by design: reading
//! `.loom/work/user.token`, because that one credential authorizes every User RPC,
//! and writing `.loom/work/stages/<id>.md`, because stage state belongs to the
//! daemon. Between them they used to leave an agent that had finished its work
//! — or found it could not finish — with no way to say so. Peer identity
//! (`peer_identity.rs`) reopened that door for `CompleteStage`; this module is
//! where the policy for widening it to blocking, disputing, freezing and
//! reading the stage's change fingerprint lives, so that widening it further
//! is a deliberate edit in one place.
//!
//! Two separate questions, kept apart because neither answer implies the other:
//!
//! * *May the connection itself authorize this request?* —
//!   [`self_service_session`]. Everything outside the listed variants answers
//!   `None` and is refused without a valid token, so a User request added later
//!   is refused by default rather than silently inheriting the peer-identity
//!   path.
//! * *Does the named session actually own the named stage?* —
//!   [`session_owns_stage_as`], read from `.loom/work/`. Peer identity proves
//!   the caller IS session A; only this proves stage X is A's to act on.
//!   Without it, a live agent could reach across into another stage.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::daemon::protocol::Request;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::parser::frontmatter::parse_from_markdown;
use crate::verify::transitions::load_stage;

/// Upper bound on a session record, matching the other readers of the same
/// files (`peer_identity`, `control_complete`).
const MAX_SESSION_FILE_BYTES: usize = 1024 * 1024;

/// The session a request claims to be running inside, for the RPCs a
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
        | Request::RecordCompletionEvidence { session_id, .. }
        | Request::DisputeCriteria { session_id, .. }
        | Request::FileDispute { session_id, .. }
        | Request::BlockStage { session_id, .. }
        | Request::FreezeContracts { session_id, .. }
        | Request::ObserveChanges { session_id, .. } => Some(session_id),
        _ => None,
    }
}

/// Session kinds that may block their own stage over the socket: the stage's
/// agent in either of its phases, the same kinds the relay matrix lets block.
const BLOCK_KINDS: &[SessionType] = &[SessionType::Stage, SessionType::Contract];

/// Disputing a criterion, findings, a contract or integrity events stays with
/// the stage session; a contract session may not dispute.
const DISPUTE_KINDS: &[SessionType] = &[SessionType::Stage];

/// Session kinds that may ask for their own stage's change fingerprint: the
/// stage's agent in either of its phases.
const OBSERVE_KINDS: &[SessionType] = &[SessionType::Stage, SessionType::Contract];

/// The stage/session pair whose ownership must be proven before the handler
/// runs, with the session kinds admitted, or `None` when there is nothing to
/// prove.
///
/// Completion and evidence requests are deliberately absent: their handlers re-validate the
/// identical binding under the sessions-directory lock, together with the
/// `Executing` requirement that only completion imposes. Checking it here too
/// would report that failure as an authentication error and lose the handler's
/// more precise message. `FreezeContracts` is absent for the same reason: its
/// handler proves the binding for a `Contract` session and says so.
///
/// A request with an empty session id — an operator shell that authenticated
/// with the user token — has no session whose ownership could be checked, and
/// the token is what carries it.
pub(super) fn ownership_to_enforce(
    request: &Request,
) -> Option<(&str, &str, &'static [SessionType])> {
    match request {
        Request::DisputeCriteria {
            stage_id,
            session_id,
            ..
        }
        | Request::FileDispute {
            stage_id,
            session_id,
            ..
        } if !session_id.is_empty() => Some((stage_id, session_id, DISPUTE_KINDS)),
        Request::BlockStage {
            stage_id,
            session_id,
            ..
        } if !session_id.is_empty() => Some((stage_id, session_id, BLOCK_KINDS)),
        Request::ObserveChanges {
            stage_id,
            session_id,
            ..
        } if !session_id.is_empty() => Some((stage_id, session_id, OBSERVE_KINDS)),
        _ => None,
    }
}

/// Whether `session_id` is the session currently assigned to `stage_id`, and
/// is one of the `allowed` kinds.
///
/// Both directions are checked — the stage's `session` field and the session
/// record's `stage_id` — plus the session kind and [`SessionStatus::Running`],
/// so a live session cannot act on a stage that is not its own and a finished
/// session cannot act at all. Completion admits knowledge sessions as well as
/// stage sessions; see `control_complete`.
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
pub(super) fn session_owns_stage_as(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    allowed: &[SessionType],
) -> Result<()> {
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
        || !allowed.contains(&session.session_type)
        || session.status != SessionStatus::Running
    {
        bail!("request does not match the active running stage session");
    }
    Ok(())
}

#[cfg(test)]
#[path = "self_service_tests.rs"]
mod tests;
