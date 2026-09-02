//! Unit tests for [`super::live_agents_for`]: a stage's live agents must
//! exclude an adjudication session, since the judge is not an agent working
//! the stage.

use super::*;
use crate::fs::session_files::save_session;
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{Session, SessionBackendKind, SessionStatus, TerminalConfig};
use crate::orchestrator::terminal::native::write_test_pid_identity;
use tempfile::TempDir;

/// A work directory whose configured terminal lane is tmux, so liveness
/// checks never run real terminal detection (which fails on a headless test
/// runner).
fn work_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    write_terminal_config(
        temp.path(),
        &TerminalConfig {
            backend: SessionBackendKind::Tmux,
        },
    )
    .unwrap();
    temp
}

fn session_for(stage_id: &str, status: SessionStatus) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = status;
    session
}

/// The PID file the wrapper script writes at spawn, naming this test process
/// so the identity probe answers `VerifiedAlive`.
fn spawn_a_live_agent(work: &std::path::Path, session: &Session) {
    write_test_pid_identity(work, session, std::process::id()).unwrap();
}

#[test]
fn live_agents_for_excludes_a_live_adjudication_session() {
    let temp = work_dir();
    let work = temp.path();

    let stage_session = session_for("alpha", SessionStatus::Running);
    spawn_a_live_agent(work, &stage_session);
    save_session(&stage_session, work).unwrap();

    let mut adjudication = Session::new_adjudication("alpha");
    adjudication.status = SessionStatus::Running;
    spawn_a_live_agent(work, &adjudication);
    save_session(&adjudication, work).unwrap();

    let agents = live_agents_for(work, "alpha").unwrap();
    assert_eq!(
        agents.len(),
        1,
        "the adjudication session must not count as a live agent"
    );
    match &agents[0] {
        LiveAgent::Known(session) => assert_eq!(session.id, stage_session.id),
        LiveAgent::Orphan(_) => panic!("expected the stage's own session, not an orphan"),
    }
}
