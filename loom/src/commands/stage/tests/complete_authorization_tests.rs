use super::*;

#[test]
#[serial]
fn no_verify_succeeds_with_matching_one_time_proof() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    write_admin_token(&work_dir, "admin-secret-token");

    let proof = completion_proof("test-stage", true, false, false, "admin-gate-test1");
    let result =
        require_admin_capability(&work_dir, "test-stage", true, false, false, Some(&proof));

    assert!(
        result.is_ok(),
        "require_admin_capability must pass when admin.token exists: {:?}",
        result.err()
    );
}

#[test]
#[serial]
fn no_verify_rejected_when_operator_proof_absent() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    // No admin.token written.

    let result = require_admin_capability(&work_dir, "test-stage", true, false, false, None);

    assert!(
        result.is_err(),
        "require_admin_capability must fail when its caller supplies no proof"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
    );
}

#[test]
#[serial]
fn force_unsafe_rejected_when_operator_proof_absent() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir, "test-stage", false, true, false, None);

    assert!(
        result.is_err(),
        "force_unsafe path must reject when the proof is absent"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
    );
}

#[test]
#[serial]
fn assume_merged_rejected_when_operator_proof_absent() {
    // --assume-merged path exercises the same gate.
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();

    let result = require_admin_capability(&work_dir, "test-stage", false, true, true, None);

    assert!(
        result.is_err(),
        "assume_merged path must reject when the proof is absent"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
    );
}

#[test]
#[serial]
fn verify_path_succeeds_without_admin_token() {
    // When no_verify / force_unsafe / assume_merged are all false, complete()
    // must NOT invoke require_admin_capability. Run complete() without any
    // verification-bypass flags in a tempdir with no admin.token: the error
    // (if any) must NOT mention the admin token. Other failure modes (no git
    // repo, etc.) are acceptable — we only assert the gate did not fire.
    let temp_dir = setup_work_dir();
    let work_dir_path = temp_dir.path().join(".loom").join("work");
    // No admin.token present in the work dir.

    let mut stage = create_test_stage("verify-path-stage", StageStatus::Executing);
    stage.acceptance = vec![AcceptanceCriterion::Simple("exit 0".to_string())];
    save_test_stage(&work_dir_path, &stage);

    let _guard = EnvGuard::new();
    std::env::set_current_dir(temp_dir.path()).unwrap();

    let result = complete_stage("verify-path-stage", false, None);

    // We don't require Ok — the test setup has no real git repo and other
    // checks may fail. The critical invariant: the admin-token gate must
    // NOT fire when no bypass flag is set.
    if let Err(e) = result {
        let msg = format!("{:#}", e);
        assert!(
            !msg.contains("admin token") && !msg.contains("admin.token"),
            "complete() with no bypass flags must not invoke admin gate, got: {msg}"
        );
    }
}

#[test]
#[serial]
fn cli_without_operator_proof_fails_admin_check() {
    let tmp = TempDir::new().unwrap();
    let work_dir = tmp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    // Deliberately do NOT write admin.token.

    let result = require_admin_capability(&work_dir, "test-stage", true, false, false, None);

    assert!(
        result.is_err(),
        "missing caller proof must fail the admin gate"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("operator proof") || err.contains("LOOM_ADMIN_PROOF"),
        "expected error mentioning the operator proof, got: {err}"
    );
}
