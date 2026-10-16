//! Client connection handling.

use super::super::protocol::{
    read_request_body, read_request_length, read_request_preface, write_message, Capability,
    Request, RequestPreface, Response,
};
use super::admission::{ByteBudget, DeadlineReader};
use super::self_service;
use super::stage_control::serve_stage_control;
use super::tokens::verify_user_token;
use anyhow::Result;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Write timeout applied to each subscriber stream clone at subscription time.
/// The status/log broadcaster holds the subscriber mutex while writing to every
/// subscriber. Without a write timeout, a subscriber that stops reading (e.g. a
/// TUI suspended with Ctrl+Z) fills its socket buffer and blocks the broadcaster
/// forever while holding the lock, stalling all status updates and every new
/// `SubscribeStatus`/`SubscribeLogs` (O-15). With a timeout, the blocked write
/// fails and `retain_mut` in the broadcaster drops that subscriber.
const SUBSCRIBER_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Read timeout applied to each accepted client connection (O-21).
/// A client that connects but never sends a complete request — or dribbles bytes
/// to keep the connection nominally alive — otherwise pins its handler thread and
/// a slot in the `CLIENT_WORKERS`/`CLIENT_QUEUE_CAPACITY` admission limits forever,
/// eventually starving legitimate clients (including `Stop`). Five seconds is generous for
/// any real request: the CLI sends one immediately on connect.
const CLIENT_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Per-stream subscriber cap. Subscriber clones outlive their short request
/// handlers, so they need an explicit bound in addition to the worker queue.
const MAX_SUBSCRIBERS_PER_STREAM: usize = 32;

/// Outcome of checking a request's credential, before its body is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Authorization {
    /// The credential is valid for the capability claimed.
    Granted,
    /// No valid credential, but the peer may still be entitled to act on its
    /// OWN stage. Only the requests `self_service::self_service_session` names
    /// may proceed on this, and only after
    /// `peer_identity::caller_is_inside_session` confirms the caller is running
    /// inside the session it names — which needs the body, so the decision
    /// cannot be finished here.
    PendingPeerIdentity,
    Denied,
}

/// Authorize from the preface alone, which is all that has been read yet.
///
/// The `PendingPeerIdentity` outcome exists because a sandboxed stage agent
/// cannot read `.loom/work/user.token` — deliberately, since that one credential
/// authorizes every User RPC — yet it must be able to block or dispute its own
/// stage. Deferring the decision lets the caller be identified by the connection
/// instead of by a secret, without widening anything else:
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

/// Report a request whose credential did not carry it, and build the refusal.
///
/// Shared by both CREDENTIAL refusal points — the flat `Denied` before the
/// body is read, and a deferred `PendingPeerIdentity` the body cannot redeem —
/// so the two can never drift into reporting differently. A request whose
/// credential was fine but whose session does not own the stage it named is a
/// different failure and says so in its own words.
fn refuse_unauthenticated(preface: &RequestPreface) -> Response {
    eprintln!(
        "Daemon request authentication failed (capability: {:?})",
        preface.capability()
    );
    Response::AuthenticationFailed
}

/// Finish the authorization the preface had to defer, now that the body is
/// known. Returns the kernel peer fact on success or the refusal to send.
///
/// Two independent gates, and a self-service request has to pass both:
///
/// 1. *Is this caller who it says it is?* Only for `PendingPeerIdentity` — a
///    caller that presented no valid token must BE the session it names.
///    `peer_pid` comes from the kernel at `connect(2)`, so a body claiming
///    someone else's session fails the ancestry check. Listing the admissible
///    requests in `self_service_session` rather than inside each arm means a
///    new User request is refused by default instead of silently inheriting
///    this path; an empty session id can never satisfy it, which is the
///    correct outcome for a caller with neither a token nor a session.
/// 2. *Is this stage the caller's to act on?* For every authorization outcome,
///    because a valid token is not the question: being inside session A says
///    nothing about whether stage X belongs to A.
fn authorize_body(
    stream: &UnixStream,
    work_dir: &Path,
    authorization: Authorization,
    preface: &RequestPreface,
    request: &Request,
) -> std::result::Result<Option<u32>, Response> {
    let peer_pid = super::peer_identity::peer_pid(stream);
    if authorization == Authorization::PendingPeerIdentity {
        let Some(session_id) = self_service::self_service_session(request) else {
            return Err(refuse_unauthenticated(preface));
        };
        let inside = peer_pid.is_some_and(|caller| {
            super::peer_identity::caller_is_inside_session(work_dir, session_id, caller)
        });
        if !inside {
            eprintln!("Request refused: caller is not inside session '{session_id}'");
            return Err(refuse_unauthenticated(preface));
        }
    }
    if let Some((stage_id, session_id, kinds)) = self_service::ownership_to_enforce(request) {
        if let Err(error) =
            self_service::session_owns_stage_as(work_dir, stage_id, session_id, kinds)
        {
            eprintln!(
                "Request refused: session '{session_id}' does not own stage '{stage_id}': {error:#}"
            );
            return Err(Response::AuthenticationFailed);
        }
    }
    Ok(peer_pid)
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

fn complete(
    preface: &RequestPreface,
    authorization: Authorization,
    peer_pid: Option<u32>,
    request: Request,
    work_dir: &Path,
) -> Response {
    let auth = super::completion_dispatch::AuthorizedCompletion::from_authenticated_connection(
        preface.capability(),
        authorization == Authorization::Granted,
        peer_pid,
    );
    super::completion_dispatch::dispatch(&auth, request, work_dir)
}

/// Puts an accepted stream into the mode the handler needs. `false` means the
/// peer has already hung up and there is nothing to serve.
fn prepare_client_stream(stream: &UnixStream) -> Result<bool> {
    // On macOS, accepted streams from a non-blocking listener may inherit
    // non-blocking mode, making read_message fail with WouldBlock.
    stream.set_nonblocking(false)?;
    match stream.set_write_timeout(Some(CLIENT_READ_TIMEOUT)) {
        Ok(()) => Ok(true),
        // macOS answers setsockopt with EINVAL once the peer has closed (a
        // status probe that hung up).
        Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(false),
        Err(error) => Err(error.into()),
    }
}

#[path = "client_dispatch.rs"]
mod dispatch;

struct AuthorizedRequest {
    preface: RequestPreface,
    authorization: Authorization,
    peer_pid: Option<u32>,
    request: Request,
    _permit: super::admission::BytePermit,
}

fn receive_request(
    stream: &mut UnixStream,
    work_dir: &Path,
    byte_budget: &Arc<ByteBudget>,
) -> Result<Option<AuthorizedRequest>> {
    let mut reader = DeadlineReader::new(&*stream, CLIENT_READ_TIMEOUT);
    let Ok(preface) = read_request_preface(&mut reader) else {
        return Ok(None);
    };
    let authorization = authorize_preface(work_dir, &preface);
    if authorization == Authorization::Denied {
        write_message(stream, &refuse_unauthenticated(&preface))?;
        return Ok(None);
    }
    let Ok(length) = read_request_length(&mut reader) else {
        return Ok(None);
    };
    let Some(permit) = byte_budget.try_reserve(length) else {
        write_message(stream, &capacity_exhausted())?;
        return Ok(None);
    };
    let request = match read_request_body(&mut reader, length) {
        Ok(request) if preface.matches(&request) => request,
        Ok(_) => {
            write_message(stream, &Response::AuthenticationFailed)?;
            return Ok(None);
        }
        Err(_) => return Ok(None),
    };
    let peer_pid = match authorize_body(stream, work_dir, authorization, &preface, &request) {
        Ok(peer_pid) => peer_pid,
        Err(refusal) => {
            write_message(stream, &refusal)?;
            return Ok(None);
        }
    };
    Ok(Some(AuthorizedRequest {
        preface,
        authorization,
        peer_pid,
        request,
        _permit: permit,
    }))
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
    if !prepare_client_stream(&stream)? {
        return Ok(());
    }
    let dispatch = dispatch::ClientDispatch {
        shutdown_flag: &shutdown_flag,
        status_subscribers: &status_subscribers,
        log_subscribers: &log_subscribers,
        work_dir,
    };
    while let Some(request) = receive_request(&mut stream, work_dir, &byte_budget)? {
        if !dispatch.serve(&mut stream, request)? {
            break;
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

#[cfg(test)]
#[path = "tests/self_service_client.rs"]
mod self_service_tests;
