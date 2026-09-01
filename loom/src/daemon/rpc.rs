//! One-shot request/response over the daemon's Unix socket, for CLI clients.
//!
//! Several `loom stage` commands change state that belongs to the daemon
//! rather than to the caller's `.work/`, and each needs the same three things:
//! a credential the caller may well be unable to read, the identity of the
//! session it is running inside, and a bounded connect-write-read. Keeping
//! them together means a fix to any of the three is a fix for all of them.

use std::io::ErrorKind;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::protocol::{read_message, write_message, Request, Response};
use super::read_user_token;

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
/// worktree, where `.work` is a symlink and the safe reader opens the work-dir
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

fn socket_path(work_dir: &Path) -> PathBuf {
    work_dir.join("orchestrator.sock")
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
    /// Nothing is listening: either no socket file exists, or one does but
    /// nothing is bound to it — the signature a daemon leaves behind when it
    /// dies without unlinking its socket (a crash, `SIGKILL`, power loss). A
    /// unix socket file outlives the process that bound it, so existence
    /// alone never proves liveness.
    NotListening,
    /// The sandbox denies AF_UNIX outright, so this process cannot reach a
    /// daemon that may well be running. Not evidence about the daemon.
    ///
    /// A caller here must not take the `NotListening` fallback: writing
    /// `.work/stages/<id>.md` directly would BYPASS a live daemon's authority
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
/// An absent socket FILE answers the question before any syscall is made, and
/// has to: a sandbox denies AF_UNIX at `socket()` creation, before the path is
/// ever considered, so without this pre-check "no daemon is configured at all"
/// and "a daemon I cannot reach" both come back `PermissionDenied` and become
/// indistinguishable — the exact difference that decides between the direct
/// write and the spool.
///
/// This is NOT the inference `daemon/server/core.rs` warns against. That
/// warning is about the opposite direction: after a FAILED connect, do not use
/// `exists()` to conclude the daemon is absent, because a sandbox that denies
/// `connect` may deny `stat` too and a false `exists()` would prove nothing.
/// As a pre-check the reasoning runs the safe way round — a socket file that
/// is genuinely absent means no daemon, and if a sandbox also denies the
/// `stat`, the resulting `NotListening` sends the caller down the direct-write
/// path, which that sandbox then refuses on its own terms. A worse error
/// message, never a wrong state change.
///
/// The connect-error mapping lives here, and only here:
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
    if !socket_path.exists() {
        return Ok(DaemonReach::NotListening);
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
             (a sandboxed environment denies AF_UNIX outright), so whether a daemon is \
             running cannot be determined from here",
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
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn a_missing_user_token_yields_the_peer_identity_placeholder() {
        let temp = TempDir::new().unwrap();

        // Non-empty, because the wire preface refuses to frame an empty
        // credential — and not the token, because there is none to read.
        assert_eq!(PEER_IDENTITY_CREDENTIAL, user_credential(temp.path()));
    }

    #[test]
    fn a_readable_user_token_is_presented_verbatim() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("user.token"), "user-secret\n").unwrap();

        assert_eq!("user-secret", user_credential(temp.path()));
    }

    fn ping() -> Request {
        Request::Ping {
            auth_token: "irrelevant".to_string(),
        }
    }

    /// Whether this process may BIND an AF_UNIX listener.
    ///
    /// Binding and connecting are governed separately on macOS: inside the
    /// Claude Code Bash sandbox (Seatbelt), `connect` behaves the same as an
    /// unsandboxed process, but `bind` fails outright with `PermissionDenied`.
    /// So a test that needs an actual listener (not just a connect attempt)
    /// must probe `bind` itself rather than reuse the connect-based check.
    fn af_unix_bind_available() -> bool {
        let temp = TempDir::new().unwrap();
        match std::os::unix::net::UnixListener::bind(temp.path().join("probe.sock")) {
            Ok(_) => true,
            Err(e) => e.kind() != ErrorKind::PermissionDenied,
        }
    }

    /// Whether this sandbox denies AF_UNIX `connect` outright.
    ///
    /// Outside any sandbox, connecting to a path with nothing bound answers
    /// `NotFound`, and connecting to a plain (non-socket) file answers
    /// `ENOTSOCK` on macOS (XNU's `unp_connect` rejects a non-socket path;
    /// Linux answers `ECONNREFUSED` for the same case instead). Only a
    /// sandbox that denies the `connect()` syscall itself answers
    /// `PermissionDenied`, which is what this probes for.
    fn af_unix_connect_denied() -> bool {
        let temp = TempDir::new().unwrap();
        matches!(
            UnixStream::connect(temp.path().join("probe.sock")),
            Err(e) if e.kind() == ErrorKind::PermissionDenied
        )
    }

    /// Deliberately NOT guarded by either probe above: the pre-check answers
    /// this before any syscall, so it must hold identically sandboxed and not.
    /// That equivalence is the whole point of the pre-check.
    #[test]
    fn no_socket_file_at_all_is_not_listening() {
        let temp = TempDir::new().unwrap();

        match try_send_request(temp.path(), &ping()).unwrap() {
            DaemonReach::NotListening => {}
            DaemonReach::Answered(response) => panic!("expected NotListening, got {response:?}"),
            DaemonReach::Unreachable => panic!("expected NotListening, got Unreachable"),
        }
    }

    #[test]
    fn a_stale_socket_file_with_nothing_bound_is_not_listening() {
        if !af_unix_bind_available() {
            // Sandboxed: bind is denied before a real stale socket could even
            // be produced, so this test can't say anything here.
            return;
        }
        let temp = TempDir::new().unwrap();
        // A REAL stale socket, not a plain file: a plain file answers
        // ENOTSOCK on macOS (XNU's unp_connect rejects a non-socket path)
        // rather than the ECONNREFUSED a dead daemon actually produces.
        // Binding and immediately dropping the listener leaves a
        // socket-typed file on disk with nothing accepting on it, which is
        // exactly what a daemon that died without unlinking its socket
        // leaves behind.
        drop(std::os::unix::net::UnixListener::bind(socket_path(temp.path())).unwrap());

        match try_send_request(temp.path(), &ping()).unwrap() {
            DaemonReach::NotListening => {}
            DaemonReach::Answered(response) => panic!("expected NotListening, got {response:?}"),
            DaemonReach::Unreachable => panic!("expected NotListening, got Unreachable"),
        }
    }

    /// The inverse of the guard above: this one can ONLY say something where
    /// AF_UNIX connect is denied, which is exactly the sandboxed stage agent
    /// this variant exists for. Under a normal environment the same connect
    /// fails `ENOTSOCK` against a plain file (or `ConnectionRefused` against
    /// a real stale socket), which the other tests already cover.
    ///
    /// The socket file has to exist, or the pre-check would answer
    /// `NotListening` before the connect this test is about ever runs.
    #[test]
    fn a_sandbox_denying_af_unix_is_unreachable_not_not_listening() {
        if !af_unix_connect_denied() {
            return;
        }
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("orchestrator.sock"), b"").unwrap();

        match try_send_request(temp.path(), &ping()).unwrap() {
            DaemonReach::Unreachable => {}
            DaemonReach::NotListening => {
                panic!("a denied socket syscall says nothing about whether a daemon is listening")
            }
            DaemonReach::Answered(response) => panic!("expected Unreachable, got {response:?}"),
        }
    }

    #[test]
    fn a_live_listener_is_answered() {
        if !af_unix_bind_available() {
            // Sandboxed: bind fails PermissionDenied before any
            // socket-specific behavior runs, so this test can't say
            // anything here.
            return;
        }
        let temp = TempDir::new().unwrap();
        let listener = std::os::unix::net::UnixListener::bind(socket_path(temp.path())).unwrap();

        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _request: Request = read_message(&mut stream).unwrap();
            write_message(&mut stream, &Response::Pong).unwrap();
        });

        match try_send_request(temp.path(), &ping()).unwrap() {
            DaemonReach::Answered(Response::Pong) => {}
            DaemonReach::Answered(other) => panic!("expected Pong, got {other:?}"),
            DaemonReach::NotListening => panic!("expected Answered, got NotListening"),
            DaemonReach::Unreachable => panic!("expected Answered, got Unreachable"),
        }

        handle.join().unwrap();
    }
}
