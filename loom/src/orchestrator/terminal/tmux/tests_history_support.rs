//! Fixtures for the history tests that persist a running session.

use super::*;
use crate::test_stand_in::StandIn;

/// A tmux session of `session` persisted as running, so the monitor samples it.
/// The returned stand-in is the agent process whose pid the session records.
/// `kill_session` terminates that pid, so it must never be the test process
/// itself: SIGTERM to the test binary ends the whole run (exit 130 where a
/// dry-run test installed `ctrlc`'s handler first). The caller runs
/// `require_signals!()` first, as every stand-in's test does.
pub(super) fn persist_running(work: &Path, session: &mut Session) -> StandIn {
    let agent = StandIn::direct(std::process::Command::new("sleep").arg("300"));
    let pid = agent.id();
    session.status = crate::models::session::SessionStatus::Running;
    session.pid = Some(pid);
    crate::orchestrator::terminal::launch::write_test_pid_identity(work, session, pid).unwrap();
    crate::fs::session_files::save_session(session, work).unwrap();
    agent
}
