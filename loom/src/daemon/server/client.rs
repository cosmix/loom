//! Client connection handling.

use super::super::protocol::{read_message, write_message, Capability, Request, Response};
use anyhow::Result;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Filename for the user-readable token (mode 0o644, mounted into containers).
pub(super) const USER_TOKEN_FILE: &str = "user.token";

/// Filename for the host-only admin token (mode 0o600, never mounted into containers).
pub(super) const ADMIN_TOKEN_FILE: &str = "admin.token";

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
    read_token_file(&work_dir.join(ADMIN_TOKEN_FILE))
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
fn token_path_for(work_dir: &Path, capability: Capability) -> PathBuf {
    match capability {
        Capability::User => work_dir.join(USER_TOKEN_FILE),
        Capability::Admin => work_dir.join(ADMIN_TOKEN_FILE),
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
                if let Ok(stream_clone) = stream.try_clone() {
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
                } else {
                    write_message(
                        &mut stream,
                        &Response::Error {
                            message: "Failed to clone stream".to_string(),
                        },
                    )?;
                }
            }
            Request::SubscribeLogs { .. } => {
                if let Ok(stream_clone) = stream.try_clone() {
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
                } else {
                    write_message(
                        &mut stream,
                        &Response::Error {
                            message: "Failed to clone stream".to_string(),
                        },
                    )?;
                }
            }
            Request::Unsubscribe { .. } => {
                write_message(&mut stream, &Response::Ok)?;
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
        std::fs::write(tmp.path().join(USER_TOKEN_FILE), "user-secret").unwrap();
        std::fs::write(tmp.path().join(ADMIN_TOKEN_FILE), "admin-secret").unwrap();
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
        std::fs::write(tmp.path().join(ADMIN_TOKEN_FILE), "admin-secret").unwrap();
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
