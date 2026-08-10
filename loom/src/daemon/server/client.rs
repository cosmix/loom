//! Client connection handling.

use super::super::protocol::{
    read_request_body, read_request_length, read_request_preface, write_message, Capability,
    Request, RequestPreface, Response,
};
use super::admission::{ByteBudget, DeadlineReader};
use anyhow::Result;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[path = "control_complete.rs"]
mod control_complete;

/// Write timeout applied to each subscriber stream clone at subscription time.
///
/// The status/log broadcaster holds the subscriber mutex while writing to every
/// subscriber. Without a write timeout, a subscriber that stops reading (e.g. a
/// TUI suspended with Ctrl+Z) fills its socket buffer and blocks the broadcaster
/// forever while holding the lock, stalling all status updates and every new
/// `SubscribeStatus`/`SubscribeLogs` (O-15). With a timeout, the blocked write
/// fails and `retain_mut` in the broadcaster drops that subscriber.
const SUBSCRIBER_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Read timeout applied to each accepted client connection (O-21).
///
/// A client that connects but never sends a complete request — or dribbles bytes
/// to keep the connection nominally alive — otherwise pins its handler thread and
/// a slot in the `CLIENT_WORKERS`/`CLIENT_QUEUE_CAPACITY` admission limits forever,
/// eventually starving legitimate clients (including `Stop`). Five seconds is generous for
/// any real request: the CLI sends one immediately on connect.
const CLIENT_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Per-stream subscriber cap. Subscriber clones outlive their short request
/// handlers, so they need an explicit bound in addition to the worker queue.
const MAX_SUBSCRIBERS_PER_STREAM: usize = 32;

/// Filename for the user-tier token (mode 0o600, lives under `.work/`).
pub(super) const USER_TOKEN_FILE: &str = "user.token";

/// Filename for the admin token (mode 0o600). Lives under the per-project
/// `.work/` directory alongside `user.token`. It is owner-only so a
/// stage-confined agent cannot read it, and being per-project means
/// concurrent daemons for different projects never share — let alone
/// clobber or delete — each other's token.
pub(super) const ADMIN_TOKEN_FILE: &str = "admin.token";

/// Path to the per-project admin token: `<work_dir>/admin.token`.
///
/// Mode 0o600 (owner-only rw). Kept per-project rather than in a shared
/// runtime directory so two daemons (different projects, or a restart)
/// can never overwrite or delete one another's token.
pub fn admin_token_path(work_dir: &Path) -> PathBuf {
    work_dir.join(ADMIN_TOKEN_FILE)
}

fn read_token_file(work_dir: &Path, relative: &Path) -> Option<String> {
    crate::fs::safe_read::read_to_string_bounded(work_dir, relative, 4096)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Read the user-tier auth token (Ping / Subscribe / Unsubscribe).
pub fn read_user_token(work_dir: &Path) -> Option<String> {
    read_token_file(work_dir, Path::new(USER_TOKEN_FILE))
}

/// Back-compat shim used by status UI helpers — returns the user token.
///
/// Kept on the public surface because TUI code reads it for `Ping` /
/// `SubscribeStatus`. Never use this for `Stop`; that path must call
/// the admin-proof verifier.
pub fn read_auth_token(work_dir: &Path) -> Option<String> {
    read_user_token(work_dir)
}

/// Constant-time comparison of two strings.
fn ct_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.as_bytes()
            .iter()
            .zip(b.as_bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn verify_user_token(work_dir: &Path, provided_token: &str) -> bool {
    let Some(expected) = read_user_token(work_dir) else {
        return false;
    };
    ct_eq(&expected, provided_token)
}

/// Outcome of checking a request's credential, before its body is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Authorization {
    /// The credential is valid for the capability claimed.
    Granted,
    /// No valid credential, but the peer may still be entitled to complete its
    /// OWN stage. Only [`Request::CompleteStage`] may proceed on this, and only
    /// after `peer_identity::caller_is_inside_session` confirms the caller is
    /// running inside the session it names — which needs the body, so the
    /// decision cannot be finished here.
    PendingPeerIdentity,
    Denied,
}

/// Authorize from the preface alone, which is all that has been read yet.
///
/// The `PendingPeerIdentity` outcome exists because a sandboxed stage agent
/// cannot read `.work/user.token` — deliberately, since that one credential
/// authorizes every User RPC — yet completing its own stage is precisely what
/// it is supposed to do. Deferring the decision lets the caller be identified
/// by the connection instead of by a secret, without widening anything else:
/// every other User request still needs the token.
fn authorize_preface(work_dir: &Path, preface: &RequestPreface) -> Authorization {
    match preface.capability() {
        Capability::User => {
            if verify_user_token(work_dir, preface.credential()) {
                Authorization::Granted
            } else {
                Authorization::PendingPeerIdentity
            }
        }
        Capability::Admin => {
            if crate::commands::stage::admin_proof::verify_and_consume_admin_proof(
                work_dir,
                crate::commands::stage::admin_proof::AdminProofRequest::daemon_stop(),
                Some(preface.credential()),
            )
            .is_ok()
            {
                Authorization::Granted
            } else {
                Authorization::Denied
            }
        }
    }
}

/// Refusal for a request whose declared length the daemon has no budget for.
fn capacity_exhausted() -> Response {
    Response::Error {
        message: "Daemon request capacity is exhausted".to_string(),
    }
}

/// Report and refuse a request whose credential did not carry it.
///
/// Shared by both refusal points — the flat `Denied` before the body is read,
/// and a deferred `PendingPeerIdentity` on any request other than
/// `CompleteStage` — so the two can never drift into reporting differently.
fn refuse_unauthenticated(stream: &mut UnixStream, preface: &RequestPreface) -> Result<()> {
    eprintln!(
        "Daemon request authentication failed (capability: {:?})",
        preface.capability()
    );
    write_message(stream, &Response::AuthenticationFailed)
}

/// Serve one `CompleteStage`, finishing the authorization the preface deferred.
///
/// A caller that presented no valid token must BE the session it names.
/// `peer_pid` comes from the kernel at `connect(2)`, so a request body claiming
/// someone else's session fails the ancestry check — the cross-stage
/// escalation that would otherwise replace the token requirement with nothing.
fn serve_complete_stage(
    stream: &UnixStream,
    work_dir: &Path,
    authorization: Authorization,
    stage_id: &str,
    session_id: &str,
    nonce: &str,
) -> Response {
    if authorization == Authorization::PendingPeerIdentity
        && !super::peer_identity::peer_pid(stream).is_some_and(|caller| {
            super::peer_identity::caller_is_inside_session(work_dir, session_id, caller)
        })
    {
        eprintln!("Completion refused: caller is not inside session '{session_id}'");
        return Response::AuthenticationFailed;
    }
    match control_complete::handle_complete_stage(work_dir, stage_id, session_id, nonce) {
        Ok(response) => response,
        Err(error) => Response::Error {
            message: format!("Completion transition refused: {error:#}"),
        },
    }
}

/// Clone a client stream for use as a broadcast subscriber, applying a write
/// timeout so a stalled subscriber cannot freeze the broadcaster (O-15).
///
/// Returns the configured clone, or a human-readable error message suitable for
/// a [`Response::Error`] if cloning or setting the timeout fails.
fn prepare_subscriber_clone(stream: &UnixStream) -> std::result::Result<UnixStream, String> {
    let clone = stream
        .try_clone()
        .map_err(|e| format!("Failed to clone stream: {e}"))?;
    clone
        .set_write_timeout(Some(SUBSCRIBER_WRITE_TIMEOUT))
        .map_err(|e| format!("Failed to set subscriber write timeout: {e}"))?;
    Ok(clone)
}

/// Handle a client connection.
pub fn handle_client_connection(
    mut stream: UnixStream,
    shutdown_flag: Arc<AtomicBool>,
    status_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    log_subscribers: Arc<Mutex<Vec<UnixStream>>>,
    work_dir: &Path,
    byte_budget: Arc<ByteBudget>,
) -> Result<()> {
    // Ensure stream is in blocking mode - on macOS, accepted streams from
    // a non-blocking listener may inherit non-blocking mode, causing
    // read_message to fail with WouldBlock immediately.
    stream.set_nonblocking(false)?;

    stream.set_write_timeout(Some(CLIENT_READ_TIMEOUT))?;

    loop {
        let mut reader = DeadlineReader::new(&stream, CLIENT_READ_TIMEOUT);
        let preface = match read_request_preface(&mut reader) {
            Ok(preface) => preface,
            Err(_) => break,
        };
        let authorization = authorize_preface(work_dir, &preface);
        if authorization == Authorization::Denied {
            refuse_unauthenticated(&mut stream, &preface)?;
            break;
        }
        let length = match read_request_length(&mut reader) {
            Ok(length) => length,
            Err(_) => break,
        };
        let Some(_permit) = byte_budget.try_reserve(length) else {
            write_message(&mut stream, &capacity_exhausted())?;
            break;
        };
        let request = match read_request_body(&mut reader, length) {
            Ok(request) if preface.matches(&request) => request,
            Ok(_) => {
                write_message(&mut stream, &Response::AuthenticationFailed)?;
                break;
            }
            Err(_) => break,
        };

        // A request that never presented a valid token gets exactly one
        // opportunity, and only the one an agent is entitled to. Listing the
        // exception here rather than inside each arm means a NEW User request
        // added later is refused by default instead of silently inheriting the
        // peer-identity path.
        if authorization == Authorization::PendingPeerIdentity
            && !matches!(request, Request::CompleteStage { .. })
        {
            refuse_unauthenticated(&mut stream, &preface)?;
            break;
        }

        match request {
            Request::Ping { .. } => {
                write_message(&mut stream, &Response::Pong)?;
            }
            Request::Stop { .. } => {
                // Capability::Admin already verified above — stop the daemon.
                write_message(&mut stream, &Response::Ok)?;
                shutdown_flag.store(true, Ordering::SeqCst);
                break;
            }
            Request::SubscribeStatus { .. } => {
                subscribe(&mut stream, &status_subscribers, "Status")?;
            }
            Request::SubscribeLogs { .. } => {
                subscribe(&mut stream, &log_subscribers, "Log")?;
            }
            Request::Unsubscribe { .. } => {
                write_message(&mut stream, &Response::Ok)?;
                break;
            }
            Request::DisputeCriteria {
                stage_id,
                criterion_index,
                reason,
                evidence_commit,
                failure_output,
                ..
            } => {
                // Daemon-side dispute handler: owns request.md persistence
                // and stage state transition to NeedsAdjudication. The
                // user.token capability check above guards entry; the
                // handler additionally validates criterion_index and the
                // dispute budget.
                let response = super::dispute::handle_dispute_criteria(
                    work_dir,
                    &stage_id,
                    criterion_index,
                    reason,
                    evidence_commit,
                    failure_output,
                )
                .unwrap_or_else(|e| Response::Error {
                    message: format!("Dispute persistence failed: {e:#}"),
                });
                write_message(&mut stream, &response)?;
                break;
            }
            Request::CompleteStage {
                stage_id,
                session_id,
                nonce,
                ..
            } => {
                let response = serve_complete_stage(
                    &stream,
                    work_dir,
                    authorization,
                    &stage_id,
                    &session_id,
                    &nonce,
                );
                write_message(&mut stream, &response)?;
                break;
            }
        }
    }

    Ok(())
}

fn subscribe(
    stream: &mut UnixStream,
    subscribers: &Mutex<Vec<UnixStream>>,
    label: &str,
) -> Result<()> {
    let stream_clone = match prepare_subscriber_clone(stream) {
        Ok(clone) => clone,
        Err(message) => return write_message(stream, &Response::Error { message }),
    };
    let added = subscribers.lock().map(|mut subscribers| {
        if subscribers.len() >= MAX_SUBSCRIBERS_PER_STREAM {
            return false;
        }
        subscribers.push(stream_clone);
        true
    });
    let response = if matches!(added, Ok(true)) {
        Response::Ok
    } else {
        Response::Error {
            message: format!("{label} subscriber capacity is exhausted"),
        }
    };
    write_message(stream, &response)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
