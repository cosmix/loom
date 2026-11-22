//! Fixtures for the history tests that persist a running session.

use super::*;

/// The stand-in for the session's agent process. `kill_session` terminates the
/// persisted pid, so it must never be the test process itself: SIGTERM to the
/// test binary ends the whole run (exit 130 where a dry-run test installed
/// `ctrlc`'s handler first).
pub(super) struct AgentProcess(std::process::Child);

impl Drop for AgentProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A tmux session of `session` persisted as running, so the monitor samples it.
/// The returned process is the pid the session records.
pub(super) fn persist_running(work: &Path, session: &mut Session) -> AgentProcess {
    let agent = AgentProcess(
        std::process::Command::new("sleep")
            .arg("300")
            .spawn()
            .unwrap(),
    );
    let pid = agent.0.id();
    session.backend = crate::models::session::SessionBackendKind::Tmux;
    session.status = crate::models::session::SessionStatus::Running;
    session.pid = Some(pid);
    crate::orchestrator::terminal::native::write_test_pid_identity(work, session, pid).unwrap();
    crate::fs::session_files::save_session(session, work).unwrap();
    agent
}
