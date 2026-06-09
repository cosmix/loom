//! Client connection handling.

use super::super::protocol::{read_message, write_message, Capability, Request, Response};
use anyhow::Result;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
/// a slot in the [`MAX_CONNECTIONS`](super::core::MAX_CONNECTIONS) cap forever,
/// eventually starving legitimate clients (including `Stop`). 30s is generous for
/// any real request: the CLI sends one immediately on connect.
const CLIENT_READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Filename for the user-readable token (mode 0o644, lives under `.work/`).
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

fn read_token_file(path: &Path) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Read the user-tier auth token (Ping / Subscribe / Unsubscribe).
pub fn read_user_token(work_dir: &Path) -> Option<String> {
    read_token_file(&work_dir.join(USER_TOKEN_FILE))
}

/// Read the admin-tier auth token (Stop).
///
/// Returns `None` if `admin.token` is missing or unreadable; callers must
/// treat that as an authentication failure rather than falling back to a
/// less-privileged token.
pub fn read_admin_token(work_dir: &Path) -> Option<String> {
    read_token_file(&admin_token_path(work_dir))
}

/// Back-compat shim used by status UI helpers — returns the user token.
///
/// Kept on the public surface because TUI code reads it for `Ping` /
/// `SubscribeStatus`. Never use this for `Stop`; that path must call
/// [`read_admin_token`] directly.
pub fn read_auth_token(work_dir: &Path) -> Option<String> {
    read_user_token(work_dir)
}

/// Path to the token file backing a given capability.
///
/// Both tokens live under the per-project `.work/` tree: `user.token`
/// (mode 0o644) and `admin.token` (mode 0o600).
fn token_path_for(work_dir: &Path, capability: Capability) -> PathBuf {
    match capability {
        Capability::User => work_dir.join(USER_TOKEN_FILE),
        Capability::Admin => admin_token_path(work_dir),
    }
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

/// Verify that `provided_token` matches the token file for `capability`.
///
/// A `Stop` request must match `admin.token` exactly; matching `user.token`
/// when admin is required returns `false`. Missing token file → `false`.
pub fn verify_for_capability(
    work_dir: &Path,
    provided_token: &str,
    capability: Capability,
) -> bool {
    let path = token_path_for(work_dir, capability);
    let Some(expected) = read_token_file(&path) else {
        return false;
    };
    ct_eq(&expected, provided_token)
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
) -> Result<()> {
    // Ensure stream is in blocking mode - on macOS, accepted streams from
    // a non-blocking listener may inherit non-blocking mode, causing
    // read_message to fail with WouldBlock immediately.
    stream.set_nonblocking(false)?;

    // Apply a read timeout so an idle or dribbling client cannot pin this thread
    // (and a slot in the 100-connection cap) indefinitely (O-21). A request that
    // does not arrive within the window causes `read_message` to error, ending
    // the handler. A subscriber that has already registered its broadcast clone
    // keeps receiving updates even after its handler thread exits, because the
    // clone is an independent fd held by the broadcaster.
    stream.set_read_timeout(Some(CLIENT_READ_TIMEOUT))?;

    loop {
        let request: Request = match read_message(&mut stream) {
            Ok(req) => req,
            Err(_) => {
                // Client disconnected or error reading
                break;
            }
        };

        // Extract the auth token and human-readable request label for logging.
        let (auth_token, request_type) = match &request {
            Request::Ping { auth_token } => (auth_token, "Ping"),
            Request::Stop { auth_token } => (auth_token, "Stop"),
            Request::SubscribeStatus { auth_token } => (auth_token, "SubscribeStatus"),
            Request::SubscribeLogs { auth_token } => (auth_token, "SubscribeLogs"),
            Request::Unsubscribe { auth_token } => (auth_token, "Unsubscribe"),
            Request::DisputeCriteria { auth_token, .. } => (auth_token, "DisputeCriteria"),
        };

        // Tag every request with the capability required to execute it, and
        // verify against the matching token file. Stop requires the admin
        // token; a Stop request bearing only the user token must fail closed.
        let required = request.required_capability();
        if !verify_for_capability(work_dir, auth_token, required) {
            eprintln!(
                "Authentication failed for {} request (required capability: {:?})",
                request_type, required
            );
            write_message(&mut stream, &Response::AuthenticationFailed)?;
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
                match prepare_subscriber_clone(&stream) {
                    Ok(stream_clone) => {
                        // Acquire lock, add subscriber, release lock before I/O
                        let lock_result = status_subscribers.lock().map(|mut subs| {
                            subs.push(stream_clone);
                        });
                        // Write response AFTER releasing the lock
                        if lock_result.is_ok() {
                            write_message(&mut stream, &Response::Ok)?;
                        } else {
                            write_message(
                                &mut stream,
                                &Response::Error {
                                    message: "Failed to acquire subscriber lock".to_string(),
                                },
                            )?;
                        }
                    }
                    Err(message) => {
                        write_message(&mut stream, &Response::Error { message })?;
                    }
                }
            }
            Request::SubscribeLogs { .. } => {
                match prepare_subscriber_clone(&stream) {
                    Ok(stream_clone) => {
                        // Acquire lock, add subscriber, release lock before I/O
                        let lock_result = log_subscribers.lock().map(|mut subs| {
                            subs.push(stream_clone);
                        });
                        // Write response AFTER releasing the lock
                        if lock_result.is_ok() {
                            write_message(&mut stream, &Response::Ok)?;
                        } else {
                            write_message(
                                &mut stream,
                                &Response::Error {
                                    message: "Failed to acquire subscriber lock".to_string(),
                                },
                            )?;
                        }
                    }
                    Err(message) => {
                        write_message(&mut stream, &Response::Error { message })?;
                    }
                }
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
                let response = match super::dispute::handle_dispute_criteria(
                    work_dir,
                    &stage_id,
                    criterion_index,
                    reason,
                    evidence_commit,
                    failure_output,
                ) {
                    Ok(resp) => resp,
                    Err(e) => Response::Error {
                        message: format!("Dispute persistence failed: {e:#}"),
                    },
                };
                write_message(&mut stream, &response)?;
                break;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn user_token_rejects_admin_request() {
        let tmp = TempDir::new().unwrap();
        // Both tokens live under the per-project work dir.
        std::fs::write(admin_token_path(tmp.path()), "admin-secret").unwrap();
        std::fs::write(tmp.path().join(USER_TOKEN_FILE), "user-secret").unwrap();

        // The user token must NOT satisfy Capability::Admin.
        assert!(!verify_for_capability(
            tmp.path(),
            "user-secret",
            Capability::Admin
        ));
        // Admin token does satisfy Admin.
        assert!(verify_for_capability(
            tmp.path(),
            "admin-secret",
            Capability::Admin
        ));
        // User token satisfies User.
        assert!(verify_for_capability(
            tmp.path(),
            "user-secret",
            Capability::User
        ));
        // Admin token does NOT satisfy User (different files).
        assert!(!verify_for_capability(
            tmp.path(),
            "admin-secret",
            Capability::User
        ));
    }

    #[test]
    fn missing_token_file_fails_closed() {
        let tmp = TempDir::new().unwrap();

        // No token files written.
        assert!(!verify_for_capability(
            tmp.path(),
            "anything",
            Capability::User
        ));
        assert!(!verify_for_capability(
            tmp.path(),
            "anything",
            Capability::Admin
        ));
    }

    #[test]
    fn empty_provided_token_fails() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(admin_token_path(tmp.path()), "admin-secret").unwrap();

        assert!(!verify_for_capability(tmp.path(), "", Capability::Admin));
    }

    #[test]
    fn stop_request_required_capability_is_admin() {
        let req = Request::Stop {
            auth_token: "ignored".to_string(),
        };
        assert_eq!(req.required_capability(), Capability::Admin);
    }

    #[test]
    fn ping_request_required_capability_is_user() {
        let req = Request::Ping {
            auth_token: "ignored".to_string(),
        };
        assert_eq!(req.required_capability(), Capability::User);
    }
}
