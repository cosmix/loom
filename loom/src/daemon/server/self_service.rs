//! What a stage agent may ask the daemon to do about its OWN stage.
//!
//! A sandboxed stage agent is denied two things by design: reading
//! `.loom/work/user.token`, because that one credential authorizes every User RPC,
//! and writing `.loom/work/stages/<id>.md`, because stage state belongs to the
//! daemon. Between them they used to leave an agent that had finished its work
//! — or found it could not finish — with no way to say so. Peer identity
//! (`peer_identity.rs`) reopened that door; this module is where the policy
//! for blocking, disputing, freezing and reading the stage's change
//! fingerprint lives, so that widening it further
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

use crate::daemon::protocol::Request;
use crate::models::session::SessionType;

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
        Request::DisputeCriteria { session_id, .. }
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
/// `FreezeContracts` is deliberately absent: its handler proves the binding
/// for a `Contract` session and says so, which reports a precise message
/// instead of an authentication error.
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
/// session cannot act at all.
///
/// Deliberately NOT checked here: [`crate::models::stage::StageStatus`]. A
/// stage may legitimately be blocked or disputed from several states, and
/// each handler validates its own transition.
///
/// This runs before the handler takes any lock, so the binding it proves could
/// in principle change before the handler mutates. That is acceptable for an
/// authorization pre-check whose handlers re-read under their own locks: the
/// window is between two facts about the same session, not a way to smuggle a
/// different one through.
pub(super) use crate::stage_service::ownership::session_owns_stage_as;

#[cfg(test)]
#[path = "self_service_tests.rs"]
mod tests;
