//! Client for the trusted PostToolUse completion transition.

use crate::daemon::{read_user_token, Request, Response};
use anyhow::Result;
use std::path::Path;

pub(super) const BROKER_ENV: &str = "LOOM_CONTROL_BROKER";

pub(super) fn broker_requested() -> bool {
    std::env::var_os(BROKER_ENV).as_deref() == Some(std::ffi::OsStr::new("1"))
}

/// Fixed non-empty stand-in used when no readable `user.token` exists.
///
/// It authorizes nothing for `CompleteStage` or `RecordCompletionEvidence` —
/// see [`completion_credential`].
const PEER_IDENTITY_CREDENTIAL: &str = "peer-identity";

/// Credential for the broker's `RecordCompletionEvidence` and `CompleteStage`
/// requests.
///
/// The broker runs in the PostToolUse hook, outside the session sandbox, so
/// it can read `.loom/work/user.token`; the session sandbox denies that read
/// to the stage agent's own commands, which is what makes the token the
/// credential that separates the trusted broker from the agent. The
/// dispatcher (`daemon/server/completion_dispatch.rs`) refuses both
/// `CompleteStage` and `RecordCompletionEvidence` with `AuthenticationFailed`
/// unless the connection authenticated with this token — peer identity alone
/// authorizes `BlockStage`/`DisputeCriteria` (see `daemon/rpc.rs`) but not
/// completion.
///
/// Inside a worktree the state directory is a symlink, and
/// `safe_open_dirfd` opens the work-dir root with `O_NOFOLLOW`, so the path
/// is canonicalized first — the same way `attestation_key`
/// (`handoff/completion/attest.rs`) reads the attestation key through the
/// same symlink.
///
/// The wire preface (`wire.rs`) refuses to frame an empty credential, so a
/// token that is still missing after that resolution falls back to this
/// non-empty placeholder. A completion request carrying the placeholder is
/// refused by the daemon's token gate; the resulting error names the missing
/// token.
pub(in crate::commands::stage) fn completion_credential(work_dir: &Path) -> String {
    let resolved = work_dir
        .canonicalize()
        .unwrap_or_else(|_| work_dir.to_path_buf());
    read_user_token(&resolved)
        .filter(|token| !token.is_empty())
        .unwrap_or_else(|| PEER_IDENTITY_CREDENTIAL.to_string())
}

pub fn request_completion(
    stage_id: &str,
    session_id: &str,
    completion_nonce: &str,
    evidence_nonce: &str,
    work_dir: &Path,
) -> Result<Response> {
    let auth_token = completion_credential(work_dir);
    let request = Request::CompleteStage {
        auth_token,
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        nonce: completion_nonce.to_string(),
        evidence_nonce: evidence_nonce.to_string(),
    };
    crate::daemon::send_request(work_dir, &request)
}

#[cfg(test)]
#[path = "tests/control_complete.rs"]
mod tests;
