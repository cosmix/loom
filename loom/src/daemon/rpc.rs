//! One-shot request/response over the daemon's Unix socket, for CLI clients.
//!
//! Several `loom stage` commands change state that belongs to the daemon
//! rather than to the caller's `.loom/work/`, and each needs the same three things:
//! a credential the caller may well be unable to read, the identity of the
//! session it is running inside, and a bounded connect-write-read. Keeping
//! them together means a fix to any of the three is a fix for all of them.

use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::protocol::{read_message, write_message, Request, Response};
use super::{read_user_token, socket_path, socket_path_fits};

/// Environment variable every loom-spawned session's wrapper exports, for all
/// session kinds. Its presence is what distinguishes an agent acting on its
/// own stage from an operator shell.
pub const SESSION_ID_ENV: &str = "LOOM_SESSION_ID";

/// How long to wait for the daemon's reply. Generous: a dispute or a block
/// takes a directory lock and writes a file, both of which can queue behind
/// the orchestrator's own state writes.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Fixed non-empty stand-in used when no readable `user.token` exists.
///
/// It authorizes nothing by itself — see [`user_credential`]. The wire preface
/// refuses to frame an empty credential, so "no token" still has to be a
/// non-empty string.
const PEER_IDENTITY_CREDENTIAL: &str = "peer-identity";

/// The credential to present for a User request.
///
/// A sandboxed worktree agent is denied the `user.token` read on purpose
/// (S-1): that one token authorizes every User RPC, not just the ones a stage
/// agent is entitled to. The read also fails by construction from inside a
/// worktree, where `.loom/work` is a symlink and the safe reader opens the work-dir
/// root with `O_NOFOLLOW`. Either way absence is the normal case here, not an
/// error.
///
/// Any credential that does not match `user.token` routes the daemon into its
/// peer-identity fallback, which authorizes exactly one thing: a caller acting
/// on the session it is actually running inside. The placeholder is what makes
/// that fallback reachable — it grants nothing on its own.
pub fn user_credential(work_dir: &Path) -> String {
    read_user_token(work_dir)
        .filter(|token| !token.is_empty())
        .unwrap_or_else(|| PEER_IDENTITY_CREDENTIAL.to_string())
}

/// The session this process is running inside, or the empty string when it is
/// not running inside one.
///
/// Empty is a truthful claim of "no session", and the daemon treats it as
/// unprovable: a caller with neither a token nor a session gets nothing.
pub fn current_session_id() -> String {
    std::env::var(SESSION_ID_ENV).unwrap_or_default()
}

/// What came back from trying to reach the daemon.
///
/// The distinction that matters to callers: a refusal from a live daemon is
/// an authoritative answer, while finding nothing to talk to is not an
/// answer at all — it means there is no authority to defer to.
pub enum DaemonReach {
    /// A daemon was listening and replied. Its answer stands, refusal
    /// included: a caller must not route around it.
    Answered(Response),
    /// Nothing is listening in this process's view: the socket path does not
    /// exist (`stat` says `ENOENT`), or a socket file does but nothing is
    /// bound to it — the signature a daemon leaves behind when it dies
    /// without unlinking its socket (a crash, `SIGKILL`, power loss). A unix
    /// socket file outlives the process that bound it, so existence alone
    /// never proves liveness.
    ///
    /// This is no proof that no daemon runs. A sandbox can hide a directory
    /// entirely (a tmpfs over it), so from inside one even `ENOENT` describes
    /// only the sandbox's view. A caller whose fallback needs no privilege
    /// the sandbox would deny must ask for positive evidence as well: the
    /// review observer (`verify::review::observer`) also requires the
    /// daemon's singleton lock to prove free (`DaemonServer::proven_stopped`).
    NotListening,
    /// This process cannot tell whether a daemon listens: the sandbox denies
    /// AF_UNIX outright, `stat` on the socket path fails with anything but
    /// `ENOENT` (a denied or masked path), or the resolved socket path is too
    /// long for `sun_path` (the daemon may run; this process cannot address
    /// it). Not evidence about the daemon.
    ///
    /// A caller here must not take the `NotListening` fallback: writing
    /// `.loom/work/stages/<id>.md` directly would BYPASS a live daemon's authority
    /// over the transition — precisely the write the sandbox denies. Spooling
    /// the request instead DEFERS to that authority: the daemon still decides,
    /// just later, and still attributes the request to the worktree it drained
    /// it from rather than to anything the request claims about itself. See
    /// [`crate::fs::stage_request`].
    Unreachable,
}

/// Try to reach the daemon and send one request, distinguishing "nothing is
/// listening" from every other failure so callers with a local fallback can
/// tell the two apart.
///
/// The socket path's `lstat` answers first, before any socket syscall, and
/// has to: a sandbox denies AF_UNIX at `socket()` creation, before the path
/// is ever considered, so without this pre-check "no daemon is configured at
/// all" and "a daemon I cannot reach" both come back `PermissionDenied` and
/// become indistinguishable — the exact difference that decides between the
/// direct write and the spool. Only `ENOENT` counts as absence; any other
/// `lstat` error (`EACCES`, `EPERM`, ...) is a path this process may not
/// look at, which is `Unreachable`.
///
/// This is NOT the inference `daemon/server/core.rs` warns against. That
/// warning is about the opposite direction: after a FAILED connect, do not use
/// `exists()` to conclude the daemon is absent, because a sandbox that denies
/// `connect` may deny `stat` too and a false `exists()` would prove nothing.
/// Here a denied `lstat` is never read as absence.
///
/// The spooling callers (`fs::stage_request`) act on `NotListening` without
/// further evidence, deliberately: at most they write under `.loom/work`
/// directly, which a sandbox that hides or protects that directory refuses
/// or diverts on its own terms. A worse error message, never a wrong state
/// change. A caller whose fallback needs no such privilege, such as
/// computing a value locally, must not rely on `NotListening` alone (see the
/// variant).
///
/// Between the `lstat` and the connect, a socket path that does not fit
/// `sun_path` (`socket_path_fits`) is `Unreachable`: the daemon may well be
/// running, this process just cannot address it. Only that length check makes
/// this call, so an `InvalidInput` from the connect for any other reason (an
/// interior NUL) stays an error.
///
/// The connect-error mapping of the spooling and completion clients lives
/// here; the daemon's own status, stop, TUI and web clients classify for
/// themselves:
///
/// - `ErrorKind::NotFound` (the file vanished between the check and the
///   connect) and `ErrorKind::ConnectionRefused` (a socket file exists but
///   nothing is bound to it — the stale-socket case) both mean there is no
///   daemon to defer to, so they map to `NotListening`.
/// - `ErrorKind::PermissionDenied` means the sandbox refused the syscall, not
///   that the daemon is absent: a sandboxed process is denied AF_UNIX
///   outright, failing at `socket()` or at `connect()` (see
///   `daemon/server/core.rs`). That maps to `Unreachable`, which callers must
///   treat as "no answer", not as "no daemon".
/// - Any other connect error is NOT evidence the daemon is absent either. It
///   stays an `Err`: silently falling back on it would turn a
///   misconfiguration into a state write nobody authorized.
/// - A failure after the connection is established (write, read, timeout) is
///   also always an `Err`, never `NotListening` — something was listening.
pub fn try_send_request(work_dir: &Path, request: &Request) -> Result<DaemonReach> {
    let socket_path = socket_path(work_dir);
    match std::fs::symlink_metadata(&socket_path) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(DaemonReach::NotListening),
        Err(_) => return Ok(DaemonReach::Unreachable),
    }
    if !socket_path_fits(&socket_path) {
        return Ok(DaemonReach::Unreachable);
    }
    let stream = match UnixStream::connect(&socket_path) {
        Ok(stream) => stream,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => {
            return Ok(DaemonReach::NotListening);
        }
        Err(e) if e.kind() == ErrorKind::PermissionDenied => {
            return Ok(DaemonReach::Unreachable);
        }
        Err(e) => {
            return Err(e).with_context(|| {
                format!("Failed to connect to daemon at {}", socket_path.display())
            })
        }
    };
    exchange(stream, request).map(DaemonReach::Answered)
}

/// Send one request and read the daemon's reply, for callers with no
/// fallback of their own: without a daemon there is nothing else they can do.
pub fn send_request(work_dir: &Path, request: &Request) -> Result<Response> {
    match try_send_request(work_dir, request)? {
        DaemonReach::Answered(response) => Ok(response),
        DaemonReach::NotListening => bail!(
            "Failed to connect to daemon at {}: no daemon is listening",
            socket_path(work_dir).display()
        ),
        DaemonReach::Unreachable => bail!(
            "Failed to connect to daemon at {}: this process may not use unix sockets \
             (a sandboxed environment denies AF_UNIX outright) or the socket path is too \
             long for AF_UNIX, so whether a daemon is running cannot be determined from here",
            socket_path(work_dir).display()
        ),
    }
}

/// Write the request and read back the reply over an already-connected
/// stream.
fn exchange(mut stream: UnixStream, request: &Request) -> Result<Response> {
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .context("Failed to set daemon socket read timeout")?;
    write_message(&mut stream, request).context("Failed to send request to daemon")?;
    read_message(&mut stream).context("Failed to read daemon response")
}

#[cfg(test)]
#[path = "rpc_tests.rs"]
mod tests;
