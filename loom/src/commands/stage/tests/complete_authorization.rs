//! Who may use the privileged completion flags, and when a proof is required.

use super::*;

#[test]
fn privileged_completion_needs_no_proof_when_there_is_no_daemon_credential() {
    // `admin.token` exists only while the daemon runs, so with the daemon down
    // no proof is obtainable BY ANYONE. Demanding one locks the operator out of
    // their own stopped project while protecting nothing: a stage agent
    // completes through the daemon broker (which needs the daemon) or by
    // writing the state directory's `stages/*.md` directly (which `denyWrite` forbids it), so
    // the daemon's absence removes its ability to act, not just its credential.
    let work = tempfile::TempDir::new().unwrap();
    assert!(
        !crate::commands::stage::admin_proof::admin_credential_exists(work.path()),
        "fixture must have no daemon credential"
    );

    authorize_privileged_completion("stage-a", true, false, false, None, work.path())
        .expect("an operator must be able to force-complete a project whose daemon is stopped");
}

#[test]
fn privileged_completion_still_demands_a_proof_once_a_daemon_exists() {
    // POSITIVE CONTROL for the test above, which would pass just as happily if
    // the proof requirement had been removed outright rather than made
    // conditional on a credential existing.
    let work = tempfile::TempDir::new().unwrap();
    std::fs::write(work.path().join("admin.token"), "daemon-secret").unwrap();

    let error = authorize_privileged_completion("stage-a", true, false, false, None, work.path())
        .expect_err("with a live daemon credential, a proof is still required");
    assert!(
        error.to_string().contains("operator proof"),
        "the refusal must name what is missing, got: {error}"
    );
}
