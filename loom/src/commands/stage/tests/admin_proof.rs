use super::*;
use serial_test::serial;

const SECRET: &str = "admin-secret-token";
const NONCE: &str = "0123456789abcdef";

fn setup() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("admin.token"), SECRET).unwrap();
    temp
}

#[test]
fn proof_is_bound_to_stage_action_and_flags() {
    let request = AdminProofRequest::completion("stage-a", true, false, false);
    let proof = mint_admin_proof(SECRET, request, NONCE);

    for wrong_request in [
        AdminProofRequest::completion("stage-b", true, false, false),
        AdminProofRequest::completion("stage-a", false, true, false),
        AdminProofRequest::completion("stage-a", true, false, false).with_action("stage.retry"),
    ] {
        let temp = setup();
        let error =
            verify_and_consume_admin_proof(temp.path(), wrong_request, Some(&proof)).unwrap_err();
        assert!(error.to_string().contains("invalid"));
    }
}

#[test]
fn daemon_stop_proof_is_not_a_completion_proof() {
    let temp = setup();
    let proof = mint_admin_proof(SECRET, AdminProofRequest::daemon_stop(), NONCE);
    let completion = AdminProofRequest::completion("daemon", false, false, false);

    let error = verify_and_consume_admin_proof(temp.path(), completion, Some(&proof)).unwrap_err();
    assert!(error.to_string().contains("invalid"));
}

#[test]
fn hmac_matches_rfc_4231_test_vector() {
    let key = [0x0bu8; 20];
    let actual = hmac_sha256(&key, b"Hi There");
    assert_eq!(
        hex::encode(actual),
        "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
    );
}

#[test]
fn missing_and_wrong_proofs_fail_closed() {
    let temp = setup();
    let request = AdminProofRequest::completion("stage-a", true, false, false);
    assert!(verify_and_consume_admin_proof(temp.path(), request, None).is_err());

    let wrong = mint_admin_proof("wrong-secret", request, NONCE);
    assert!(verify_and_consume_admin_proof(temp.path(), request, Some(&wrong)).is_err());
}

#[test]
fn valid_proof_is_consumed_exactly_once() {
    let temp = setup();
    let request = AdminProofRequest::completion("stage-a", true, false, false);
    let proof = mint_admin_proof(SECRET, request, NONCE);

    verify_and_consume_admin_proof(temp.path(), request, Some(&proof)).unwrap();
    #[cfg(unix)]
    assert_replay_permissions(temp.path());

    let replay = verify_and_consume_admin_proof(temp.path(), request, Some(&proof)).unwrap_err();
    assert!(replay.to_string().contains("already been used"));
}

#[cfg(unix)]
fn assert_replay_permissions(work_dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let replay_dir = work_dir.join(REPLAY_DIR);
    assert_eq!(
        std::fs::metadata(&replay_dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let marker = std::fs::read_dir(&replay_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(
        std::fs::metadata(marker).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn verifier_rejects_symlinked_token_and_replay_directory() {
    use std::os::unix::fs::symlink;

    let token_root = tempfile::tempdir().unwrap();
    let outside_token = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(outside_token.path(), SECRET).unwrap();
    symlink(outside_token.path(), token_root.path().join("admin.token")).unwrap();
    let request = AdminProofRequest::completion("stage-a", true, false, false);
    let proof = mint_admin_proof(SECRET, request, NONCE);
    assert!(
        verify_and_consume_admin_proof(token_root.path(), request, Some(&proof)).is_err(),
        "admin.token symlinks must be refused"
    );

    let replay_root = setup();
    let outside_dir = tempfile::tempdir().unwrap();
    symlink(outside_dir.path(), replay_root.path().join(REPLAY_DIR)).unwrap();
    assert!(
        verify_and_consume_admin_proof(replay_root.path(), request, Some(&proof)).is_err(),
        "replay directory symlinks must be refused"
    );
    assert_eq!(std::fs::read_dir(outside_dir.path()).unwrap().count(), 0);
}

#[test]
#[serial]
fn operator_env_mint_round_trips_without_retaining_secret() {
    let temp = setup();
    std::env::set_var(ADMIN_SECRET_ENV, SECRET);

    let proof = mint_completion_proof_from_env("stage-a", true, false, false).unwrap();

    assert!(std::env::var_os(ADMIN_SECRET_ENV).is_none());
    let request = AdminProofRequest::completion("stage-a", true, false, false);
    verify_and_consume_admin_proof(temp.path(), request, Some(&proof)).unwrap();
}

#[test]
#[serial]
fn operator_env_mints_daemon_stop_proof_without_retaining_secret() {
    let temp = setup();
    std::env::set_var(ADMIN_SECRET_ENV, SECRET);

    let proof = mint_daemon_stop_proof_from_env().unwrap();

    assert!(std::env::var_os(ADMIN_SECRET_ENV).is_none());
    verify_and_consume_admin_proof(temp.path(), AdminProofRequest::daemon_stop(), Some(&proof))
        .unwrap();
}
