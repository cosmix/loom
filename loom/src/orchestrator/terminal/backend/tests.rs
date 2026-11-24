//! Unit tests for `backend.rs`.
//!
//! `dispatch_spawn` is private, but this module is a child of `backend`, so
//! the spawn seam can be driven directly with closures instead of through the
//! public `spawn_*` wrappers (which would need a real `Stage`, `Worktree`,
//! signal file, and tmux).

use super::*;
use tempfile::TempDir;

fn failing_tmux(_tmux: &TmuxBackend, _session: Session) -> Result<Session> {
    anyhow::bail!("tmux new-session failed for socket 'loom-x': boom")
}

#[test]
fn tmux_spawn_failure_returns_err_naming_the_session() {
    let temp = TempDir::new().unwrap();
    let backend = SessionBackend::with_tmux_probe(temp.path().to_path_buf(), || true);
    let session = Session::new();
    let session_id = session.id.clone();

    let err = backend
        .dispatch_spawn(session, failing_tmux)
        .expect_err("a failing tmux spawn must return Err");

    let rendered = format!("{err:#}");
    assert!(
        rendered.contains("tmux new-session failed"),
        "the original tmux error is the caller's only useful diagnostic, got: {rendered}"
    );
    assert!(
        rendered.contains(&session_id),
        "the error must name the session that failed to spawn, got: {rendered}"
    );
    assert!(
        std::fs::read_dir(temp.path()).unwrap().next().is_none(),
        "a tmux spawn failure must write nothing to the work dir"
    );
}

#[test]
fn missing_tmux_returns_the_tmux_message_and_never_spawns() {
    let temp = TempDir::new().unwrap();
    let backend = SessionBackend::with_tmux_probe(temp.path().to_path_buf(), || false);

    let err = backend
        .dispatch_spawn(Session::new(), |_tmux, _s| {
            panic!("tmux must not be spawned when unavailable")
        })
        .expect_err("no tmux on PATH must fail");

    assert_eq!(err.to_string(), TMUX_MISSING_MESSAGE);
    assert!(
        std::fs::read_dir(temp.path()).unwrap().next().is_none(),
        "an unavailable tmux must write nothing to the work dir"
    );
}

#[test]
fn ensure_tmux_follows_the_probe() {
    let temp = TempDir::new().unwrap();
    let missing = SessionBackend::with_tmux_probe(temp.path().to_path_buf(), || false);
    let err = missing
        .ensure_tmux()
        .expect_err("no tmux on PATH must fail");
    assert_eq!(err.to_string(), TMUX_MISSING_MESSAGE);

    let present = SessionBackend::with_tmux_probe(temp.path().to_path_buf(), || true);
    assert!(present.ensure_tmux().is_ok());
}

/// The contract spawn is the stage spawn with another kind, so the missing-tmux
/// refusal reaches it before anything is spawned.
#[test]
fn contract_spawn_goes_through_the_tmux_check() {
    let temp = TempDir::new().unwrap();
    let backend = SessionBackend::with_tmux_probe(temp.path().to_path_buf(), || false);
    let worktree = Worktree::new(
        "alpha".to_string(),
        temp.path().join(".worktrees").join("alpha"),
        "loom/alpha".to_string(),
    );

    let err = backend
        .spawn_contract_session(
            &Stage::default(),
            &worktree,
            Session::new_contract("alpha"),
            &temp.path().join("signal.md"),
        )
        .expect_err("no tmux on PATH must fail");

    assert_eq!(err.to_string(), TMUX_MISSING_MESSAGE);
    assert!(std::fs::read_dir(temp.path()).unwrap().next().is_none());
}

fn ended_tmux_session() -> Session {
    let mut session = Session::new();
    session.assign_to_stage("ended-stage".to_string());
    session
}

#[test]
fn an_ended_tmux_server_that_goes_down_at_once_is_not_asked_again() {
    let attempts = std::cell::Cell::new(0);

    end_tmux_server(&ended_tmux_session(), || {
        attempts.set(attempts.get() + 1);
        Ok(())
    })
    .unwrap();

    assert_eq!(attempts.get(), 1);
}

#[test]
fn an_ended_tmux_server_whose_first_kill_fails_is_killed_on_the_second_try() {
    let attempts = std::cell::Cell::new(0);

    end_tmux_server(&ended_tmux_session(), || {
        attempts.set(attempts.get() + 1);
        if attempts.get() == 1 {
            anyhow::bail!("tmux kill-server failed");
        }
        Ok(())
    })
    .unwrap();

    assert_eq!(attempts.get(), 2);
}

#[test]
fn an_ended_tmux_server_that_survives_both_tries_names_its_session_and_socket() {
    let session = ended_tmux_session();
    let attempts = std::cell::Cell::new(0);

    let error = end_tmux_server(&session, || {
        attempts.set(attempts.get() + 1);
        anyhow::bail!("tmux kill-server failed")
    })
    .unwrap_err();

    assert_eq!(attempts.get(), 2, "one retry, not a loop");
    let message = format!("{error:#}");
    assert!(message.contains(&session.id), "{message}");
    assert!(message.contains(&tmux::socket_name(&session)), "{message}");
    assert!(message.contains("tmux kill-server failed"), "{message}");
}
