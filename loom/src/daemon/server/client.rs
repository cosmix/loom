//! Client connection handling.

use super::super::protocol::{read_message, write_message, Capability, Request, Response};
use anyhow::Result;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Filename for the user-readable token (mode 0o644, mounted into containers).
pub(super) const USER_TOKEN_FILE: &str = "user.token";

/// Filename for the host-only admin token (mode 0o600). NOT placed under
/// `.work/` (the canonical location is [`admin_token_path`] in the daemon
/// runtime dir) — `.work/` is mounted into containers, so any token kept
/// there would be reachable by container-resident agents. Retained here as
/// a single source of truth for the basename.
pub(super) const ADMIN_TOKEN_FILE: &str = "admin.token";

/// Host-only path to admin.token. Located in the daemon runtime
/// directory (XDG_RUNTIME_DIR or fallback to data_dir()) so the
/// container topology — which only mounts .work — cannot reach it.
///
/// Layout: `$XDG_RUNTIME_DIR/loom/admin.token` on Linux, falling back
/// to `~/.local/share/loom/admin.token` (or platform data_dir) when no
/// runtime dir is set. Mode 0o600 (owner-only rw).
pub fn admin_token_path() -> std::path::PathBuf {
    dirs::runtime_dir()
        .unwrap_or_else(|| dirs::data_dir().expect("HOME unset; no runtime/data dir"))
        .join("loom")
        .join(ADMIN_TOKEN_FILE)
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
///
/// `_work_dir` is retained for backwards compatibility but no longer used:
/// admin.token now lives at [`admin_token_path`] (a host-only runtime
/// directory) rather than under the project's `.work/` tree, since `.work/`
/// is mounted into containers.
pub fn read_admin_token(_work_dir: &Path) -> Option<String> {
    read_token_file(&admin_token_path())
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
/// `Capability::User` resolves to `<work_dir>/user.token` (mounted ro into
/// containers). `Capability::Admin` resolves to [`admin_token_path`] — a
/// host-only runtime path NOT reachable from inside containers; the
/// `work_dir` argument is unused for the admin case by design.
fn token_path_for(work_dir: &Path, capability: Capability) -> PathBuf {
    match capability {
        Capability::User => work_dir.join(USER_TOKEN_FILE),
        Capability::Admin => admin_token_path(),
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
    use serial_test::serial;
    use tempfile::TempDir;

    /// Test helper: redirect `dirs::runtime_dir()` (which resolves to
    /// `$XDG_RUNTIME_DIR` on Linux) to a tempdir for the duration of a test.
    /// Writes `<runtime>/loom/admin.token` with the given content.
    fn write_admin_token_at_runtime(runtime_dir: &Path, content: &str) {
        let loom_dir = runtime_dir.join("loom");
        std::fs::create_dir_all(&loom_dir).unwrap();
        std::fs::write(loom_dir.join("admin.token"), content).unwrap();
    }

    #[test]
    #[serial]
    fn user_token_rejects_admin_request() {
        let tmp = TempDir::new().unwrap();
        // Redirect admin.token lookup to the tempdir so the test does not
        // depend on the host's real XDG_RUNTIME_DIR.
        let prev_xdg = std::env::var_os("XDG_RUNTIME_DIR");
        std::env::set_var("XDG_RUNTIME_DIR", tmp.path());
        write_admin_token_at_runtime(tmp.path(), "admin-secret");

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

        match prev_xdg {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
    }

    #[test]
    #[serial]
    fn missing_token_file_fails_closed() {
        let tmp = TempDir::new().unwrap();
        // Redirect admin.token lookup to a tempdir that has no loom/ subdir.
        let prev_xdg = std::env::var_os("XDG_RUNTIME_DIR");
        std::env::set_var("XDG_RUNTIME_DIR", tmp.path());

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

        match prev_xdg {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
    }

    #[test]
    #[serial]
    fn empty_provided_token_fails() {
        let tmp = TempDir::new().unwrap();
        let prev_xdg = std::env::var_os("XDG_RUNTIME_DIR");
        std::env::set_var("XDG_RUNTIME_DIR", tmp.path());
        write_admin_token_at_runtime(tmp.path(), "admin-secret");

        assert!(!verify_for_capability(tmp.path(), "", Capability::Admin));

        match prev_xdg {
            Some(v) => std::env::set_var("XDG_RUNTIME_DIR", v),
            None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
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
