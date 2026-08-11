//! Client for the trusted PostToolUse completion transition.

use crate::daemon::{read_message, read_user_token, write_message, Request, Response};
use anyhow::{bail, Context, Result};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub(super) const BROKER_ENV: &str = "LOOM_CONTROL_BROKER";
pub(super) const VERIFIED_MARKER: &str = "LOOM_CONTROL_VERIFICATION_PASSED";

pub(super) fn broker_requested() -> bool {
    std::env::var_os(BROKER_ENV).as_deref() == Some(std::ffi::OsStr::new("1"))
}

/// Fixed non-empty stand-in used when no readable `user.token` exists.
///
/// It authorizes nothing by itself — see [`completion_credential`].
const PEER_IDENTITY_CREDENTIAL: &str = "peer-identity";

/// Credential for the `CompleteStage` request.
///
/// A sandboxed worktree agent is denied the `user.token` read on purpose
/// (S-1) — the token authorizes every User RPC, not just this one. The
/// broker itself can't read it either when run from inside a worktree:
/// `.work` there is a symlink, and `safe_open_dirfd` opens the work-dir root
/// with `O_NOFOLLOW`, so the read fails by construction. Either way, absence
/// is the normal case on this path, not an error.
///
/// The wire preface (`wire.rs`) refuses to frame an empty credential, so "no
/// token" still has to be a non-empty string. Any credential that doesn't
/// match `user.token` routes the daemon into its `Authorization::PendingPeerIdentity`
/// fallback, which authorizes exactly one thing: a caller completing the
/// session it is actually running inside, verified via socket peer
/// credentials (`peer_identity::caller_is_inside_session`). The placeholder
/// is just what makes that fallback reachable — it grants nothing on its own.
fn completion_credential(work_dir: &Path) -> String {
    read_user_token(work_dir)
        .filter(|token| !token.is_empty())
        .unwrap_or_else(|| PEER_IDENTITY_CREDENTIAL.to_string())
}

pub(super) fn send_completion(stage_id: &str, session_id: &str, work_dir: &Path) -> Result<()> {
    let auth_token = completion_credential(work_dir);
    let request = Request::CompleteStage {
        auth_token,
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        nonce: uuid::Uuid::new_v4().simple().to_string(),
    };
    let socket_path = work_dir.join("orchestrator.sock");
    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("failed to connect to daemon at {}", socket_path.display()))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .context("failed to set completion broker socket timeout")?;
    write_message(&mut stream, &request).context("failed to send CompleteStage request")?;
    let response: Response = read_message(&mut stream).context("failed to read daemon response")?;
    match response {
        Response::Ok => Ok(()),
        Response::Error { message } => bail!("daemon refused completion: {message}"),
        Response::AuthenticationFailed => bail!("daemon rejected completion broker credential"),
        other => bail!("unexpected completion broker response: {other:?}"),
    }
}

#[cfg(test)]
#[path = "tests/control_complete.rs"]
mod tests;
