use super::*;
use crate::commands::stage::admin_proof::{mint_admin_proof, AdminProofRequest};
use crate::daemon::protocol::read_message;
use crate::daemon::server::tokens::{admin_token_path, verify_user_token, USER_TOKEN_FILE};
use std::io::Cursor;
use std::sync::atomic::AtomicBool;
use std::thread;
use tempfile::TempDir;

/// Every existing caller of this alias tests an ADMIN request, whose failed
/// credential is still a flat refusal. A failed USER credential is now
/// `PendingPeerIdentity` instead — see
/// `a_bad_user_token_defers_rather_than_denying`.
const DENIED: Authorization = Authorization::Denied;

fn request_preface(request: &Request) -> RequestPreface {
    let mut bytes = Vec::new();
    write_message(&mut bytes, request).unwrap();
    read_request_preface(&mut Cursor::new(bytes)).unwrap()
}

#[test]
fn user_token_is_scoped_to_user_requests() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(USER_TOKEN_FILE), "user-secret").unwrap();

    assert!(verify_user_token(tmp.path(), "user-secret"));
    assert!(!verify_user_token(tmp.path(), "admin-secret"));
    let stop = request_preface(&Request::Stop {
        auth_token: "user-secret".to_string(),
    });
    assert_eq!(DENIED, authorize_preface(tmp.path(), &stop));
}

#[test]
fn daemon_stop_requires_an_action_bound_one_time_proof() {
    const SECRET: &str = "admin-secret";
    const NONCE: &str = "0123456789abcdef";
    let tmp = TempDir::new().unwrap();
    std::fs::write(admin_token_path(tmp.path()), SECRET).unwrap();
    let proof = mint_admin_proof(SECRET, AdminProofRequest::daemon_stop(), NONCE);
    let preface = request_preface(&Request::Stop { auth_token: proof });

    assert_eq!(
        Authorization::Granted,
        authorize_preface(tmp.path(), &preface)
    );
    assert_eq!(DENIED, authorize_preface(tmp.path(), &preface));
}

#[test]
fn missing_token_file_fails_closed() {
    let tmp = TempDir::new().unwrap();

    assert!(!verify_user_token(tmp.path(), "anything"));
    let stop = request_preface(&Request::Stop {
        auth_token: "v1:0123456789abcdef:00".to_string(),
    });
    assert_eq!(DENIED, authorize_preface(tmp.path(), &stop));
}

#[test]
fn empty_provided_token_fails() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(USER_TOKEN_FILE), "user-secret").unwrap();

    assert!(!verify_user_token(tmp.path(), ""));
}

#[test]
fn stop_request_required_capability_is_admin() {
    let request = Request::Stop {
        auth_token: "ignored".to_string(),
    };
    assert_eq!(request.required_capability(), Capability::Admin);
}

#[test]
fn ping_request_required_capability_is_user() {
    let request = Request::Ping {
        auth_token: "ignored".to_string(),
    };
    assert_eq!(request.required_capability(), Capability::User);
}

#[test]
fn valid_user_client_passes_preface_admission_and_receives_response() {
    let work_dir = TempDir::new().unwrap();
    std::fs::write(work_dir.path().join(USER_TOKEN_FILE), "user-secret").unwrap();
    let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let handler = {
        let shutdown = Arc::clone(&shutdown);
        let work_dir = work_dir.path().to_path_buf();
        thread::spawn(move || {
            handle_client_connection(
                server_stream,
                shutdown,
                Arc::new(Mutex::new(Vec::new())),
                Arc::new(Mutex::new(Vec::new())),
                &work_dir,
                ByteBudget::new(2 * crate::daemon::wire::MAX_REQUEST_BYTES),
            )
        })
    };

    write_message(
        &mut client_stream,
        &Request::Ping {
            auth_token: "user-secret".to_string(),
        },
    )
    .unwrap();
    let response: Response = read_message(&mut client_stream).unwrap();

    assert!(matches!(response, Response::Pong));
    drop(client_stream);
    handler.join().unwrap().unwrap();
    assert!(!shutdown.load(Ordering::SeqCst));
}

#[test]
fn valid_stop_proof_is_verified_before_request_and_shuts_down() {
    const SECRET: &str = "admin-secret";
    const NONCE: &str = "fedcba9876543210";
    let work_dir = TempDir::new().unwrap();
    std::fs::write(admin_token_path(work_dir.path()), SECRET).unwrap();
    let proof = mint_admin_proof(SECRET, AdminProofRequest::daemon_stop(), NONCE);
    let (server_stream, mut client_stream) = UnixStream::pair().unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let handler = {
        let shutdown = Arc::clone(&shutdown);
        let work_dir = work_dir.path().to_path_buf();
        thread::spawn(move || {
            handle_client_connection(
                server_stream,
                shutdown,
                Arc::new(Mutex::new(Vec::new())),
                Arc::new(Mutex::new(Vec::new())),
                &work_dir,
                ByteBudget::new(2 * crate::daemon::wire::MAX_REQUEST_BYTES),
            )
        })
    };

    write_message(&mut client_stream, &Request::Stop { auth_token: proof }).unwrap();
    let response: Response = read_message(&mut client_stream).unwrap();

    assert!(matches!(response, Response::Ok));
    handler.join().unwrap().unwrap();
    assert!(shutdown.load(Ordering::SeqCst));
}

#[test]
fn a_bad_user_token_defers_to_peer_identity_rather_than_denying() {
    // The deadlock this resolves: a sandboxed agent cannot read
    // `.loom/work/user.token` (it authorizes every User RPC), yet acting on its own
    // stage is the one thing it must be able to do. A failed User credential
    // therefore defers instead of refusing — but the deferral is worthless on
    // its own, since `authorize_body` admits it only for the requests
    // `self_service::self_service_session` names, and then requires the caller
    // to actually BE the session it names. See `tests/self_service_client.rs`.
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(USER_TOKEN_FILE), "user-secret").unwrap();

    let ping = request_preface(&Request::Ping {
        auth_token: "wrong".to_string(),
    });
    assert_eq!(
        Authorization::PendingPeerIdentity,
        authorize_preface(tmp.path(), &ping),
        "a User request with a bad token defers; the request-type gate refuses it later"
    );

    let good = request_preface(&Request::Ping {
        auth_token: "user-secret".to_string(),
    });
    assert_eq!(
        Authorization::Granted,
        authorize_preface(tmp.path(), &good),
        "a valid user token must still be granted outright"
    );
}

#[test]
fn a_failed_admin_credential_is_never_deferred() {
    // Admin has no peer-identity path and must never acquire one: deferring
    // it would turn a failed daemon-stop proof into an authorization attempt
    // against the process tree.
    let tmp = TempDir::new().unwrap();
    std::fs::write(admin_token_path(tmp.path()), "admin-secret").unwrap();
    let stop = request_preface(&Request::Stop {
        auth_token: "v1:0123456789abcdef:00".to_string(),
    });
    assert_eq!(Authorization::Denied, authorize_preface(tmp.path(), &stop));
}
