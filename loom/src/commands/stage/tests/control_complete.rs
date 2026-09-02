//! Credential selection for the trusted `CompleteStage` broker request.

use super::*;
use crate::daemon::{Request, WireMessage, MAX_CREDENTIAL_BYTES};

#[test]
fn uses_the_trimmed_user_token_when_one_is_readable() {
    let work = tempfile::tempdir().unwrap();
    let token = "a".repeat(64);
    std::fs::write(work.path().join("user.token"), format!("{token}\n")).unwrap();

    assert_eq!(completion_credential(work.path()), token);
}

#[test]
fn falls_back_to_the_peer_identity_placeholder_when_no_token_file_exists() {
    let work = tempfile::tempdir().unwrap();

    let credential = completion_credential(work.path());

    assert_eq!(credential, PEER_IDENTITY_CREDENTIAL);
    assert!(
        !credential.is_empty(),
        "wire preface refuses an empty credential"
    );
    assert!(
        credential.len() <= MAX_CREDENTIAL_BYTES,
        "credential must fit the wire preface's bounded length field"
    );
}

#[test]
fn falls_back_to_the_peer_identity_placeholder_when_the_token_file_is_empty() {
    let work = tempfile::tempdir().unwrap();
    std::fs::write(work.path().join("user.token"), "").unwrap();

    // Pins the `.filter(|token| !token.is_empty())` guard: `read_token_file`
    // trims to "" for a blank file, and that must not be used verbatim.
    assert_eq!(completion_credential(work.path()), PEER_IDENTITY_CREDENTIAL);
}

#[test]
fn falls_back_to_the_peer_identity_placeholder_when_the_work_dir_is_a_symlink() {
    use std::os::unix::fs::symlink;

    // Reproduces the production failure: a worktree's state directory is a
    // symlink to the real state directory, and `safe_open_dirfd` opens the
    // work-dir root with `O_NOFOLLOW`, so `read_user_token` cannot see the
    // token even though it is present on disk.
    let real_root = tempfile::tempdir().unwrap();
    let real_work = real_root.path().join("real").join(".loom").join("work");
    std::fs::create_dir_all(&real_work).unwrap();
    std::fs::write(real_work.join("user.token"), "a".repeat(64)).unwrap();

    let link_root = tempfile::tempdir().unwrap();
    let work_dir_symlink = link_root.path().join(".loom").join("work");
    std::fs::create_dir_all(link_root.path().join(".loom")).unwrap();
    symlink(&real_work, &work_dir_symlink).unwrap();

    assert_eq!(
        completion_credential(&work_dir_symlink),
        PEER_IDENTITY_CREDENTIAL
    );
}

#[test]
fn wire_preface_frames_the_placeholder_but_refuses_an_empty_credential() {
    let placeholder_request = Request::CompleteStage {
        auth_token: PEER_IDENTITY_CREDENTIAL.to_string(),
        stage_id: "stage-a".to_string(),
        session_id: "session-a".to_string(),
        nonce: "nonce-a".to_string(),
    };
    placeholder_request
        .write_wire(&mut Vec::new())
        .expect("a non-empty placeholder credential must be frameable");

    let empty_request = Request::CompleteStage {
        auth_token: String::new(),
        stage_id: "stage-a".to_string(),
        session_id: "session-a".to_string(),
        nonce: "nonce-a".to_string(),
    };
    let error = empty_request
        .write_wire(&mut Vec::new())
        .expect_err("the old unwrap_or_default() behavior must stay rejected");
    assert!(
        error.to_string().contains("credential length"),
        "expected a credential-length error, got: {error}"
    );
}
