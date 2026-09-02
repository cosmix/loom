use super::*;
use tempfile::TempDir;

/// This process is trivially "inside itself", and its parent is a real
/// ancestor — both without spawning anything.
#[test]
fn ancestry_accepts_self_and_the_real_parent_chain() {
    if crate::process::sandbox_probe::skip_unless(
        crate::process::sandbox_probe::process_tree_visible(),
        "daemon::server::peer_identity::tests::ancestry_accepts_self_and_the_real_parent_chain",
        "walking up from a real parent needs a visible process tree",
    ) {
        return;
    }
    let me = std::process::id();
    assert!(is_at_or_below(me, me), "a process is at its own pid");

    let parent = parent_pid(me).expect("this process must have a readable parent");
    assert!(
        is_at_or_below(me, parent),
        "walking up from {me} must reach its parent {parent}"
    );
}

#[test]
fn ancestry_rejects_a_process_that_is_not_an_ancestor() {
    // pid 1 is an ancestor of everything, so its PARENT direction is the
    // honest negative: nothing is below a pid that cannot be reached by
    // walking up from us. A high unused pid also cannot be on our chain.
    let me = std::process::id();
    assert!(
        !is_at_or_below(me, 4_294_967_295),
        "an unrelated pid must never be reported as an ancestor"
    );
}

#[test]
fn a_caller_naming_a_session_with_no_pid_evidence_is_refused() {
    // Fail closed on every missing piece. Without PID-file evidence there is
    // no start time, so a recycled pid could otherwise stand in for a dead
    // session and complete its stage.
    let work = TempDir::new().unwrap();
    std::fs::create_dir_all(work.path().join("sessions")).unwrap();

    let mut session = crate::models::session::Session::new();
    session.assign_to_stage("stage-a".to_string());
    session.pid = Some(std::process::id());
    crate::fs::session_files::save_session(&session, work.path()).unwrap();

    assert!(
        !caller_is_inside_session(work.path(), &session.id, std::process::id()),
        "a session with no PID identity evidence must not authorize anything"
    );
}

#[test]
fn a_caller_naming_an_unknown_session_is_refused() {
    let work = TempDir::new().unwrap();
    std::fs::create_dir_all(work.path().join("sessions")).unwrap();
    assert!(!caller_is_inside_session(
        work.path(),
        "session-does-not-exist",
        std::process::id()
    ));
}

#[test]
fn a_caller_outside_the_session_it_names_is_refused() {
    // The cross-stage escalation this exists to stop: agent A names agent B's
    // session. Everything about B is valid and live — the ONLY thing wrong is
    // that the caller is not inside it. Recorded as pid 1, which is nobody's
    // descendant-of in the walking-up direction used here.
    let work = TempDir::new().unwrap();
    let mut session = crate::models::session::Session::new();
    session.assign_to_stage("stage-b".to_string());
    session.pid = Some(1);
    crate::fs::session_files::save_session(&session, work.path()).unwrap();
    crate::orchestrator::terminal::native::write_test_pid_identity(work.path(), &session, 1)
        .unwrap();

    assert!(
        !caller_is_inside_session(work.path(), &session.id, std::process::id()),
        "naming another session must not authorize completing it"
    );
}
