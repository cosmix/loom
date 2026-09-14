//! Authenticated, transport-independent completion request dispatch.

use std::path::Path;

use crate::daemon::protocol::{Capability, Request, Response};

/// Proof that the connection handler completed credential, body, ownership,
/// and (when required) kernel peer-identity authorization for this request.
pub(super) struct AuthorizedCompletion {
    _capability: Capability,
    credential_authenticated: bool,
    _peer_pid: Option<u32>,
}

impl AuthorizedCompletion {
    /// Minted only by the real connection handler after all of its gates pass.
    pub(super) fn from_authenticated_connection(
        capability: Capability,
        credential_authenticated: bool,
        peer_pid: Option<u32>,
    ) -> Self {
        debug_assert_eq!(capability, Capability::User);
        Self {
            _capability: capability,
            credential_authenticated,
            _peer_pid: peer_pid,
        }
    }
}

pub(super) fn dispatch(auth: &AuthorizedCompletion, request: Request, work_dir: &Path) -> Response {
    let is_completion = matches!(
        &request,
        Request::CompleteStage { .. } | Request::RecordCompletionEvidence { .. }
    );
    if is_completion && !auth.credential_authenticated {
        return Response::AuthenticationFailed;
    }
    match request {
        Request::CompleteStage {
            stage_id,
            session_id,
            nonce,
            evidence_nonce,
            ..
        } => super::control_complete::handle_complete_stage(
            work_dir,
            &stage_id,
            &session_id,
            &nonce,
            &evidence_nonce,
        )
        .unwrap_or_else(|error| bounded_error("Completion transition refused", error)),
        Request::RecordCompletionEvidence {
            stage_id,
            session_id,
            evidence,
            ..
        } => super::completion_evidence::handle_record(work_dir, &stage_id, &session_id, *evidence)
            .unwrap_or_else(super::completion_evidence::refusal_response),
        _ => Response::Error {
            message: "Authenticated completion dispatcher received a non-completion request"
                .to_string(),
        },
    }
}

fn bounded_error(context: &str, error: anyhow::Error) -> Response {
    const MAX_CHARS: usize = 1024;
    let message = format!("{context}: {error:#}")
        .chars()
        .take(MAX_CHARS)
        .collect();
    Response::Error { message }
}

#[cfg(test)]
#[path = "completion_dispatch/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "completion_dispatch/tests_auth.rs"]
mod tests_auth;
