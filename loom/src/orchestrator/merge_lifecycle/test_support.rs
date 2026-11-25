//! Fixtures shared by the tests that need a stage with a live session.

use std::path::Path;

use crate::fs::session_files::save_session;
use crate::models::session::{Session, SessionStatus, SessionType};
use crate::orchestrator::terminal::launch::write_test_pid_identity;

/// Save a running `session_type` session for `stage_id` whose recorded process
/// is this test process, so liveness probes answer alive from the identity
/// file written here.
pub(crate) fn write_live_session(
    work_dir: &Path,
    stage_id: &str,
    session_type: SessionType,
) -> Session {
    let mut session = Session::new();
    session.assign_to_stage(stage_id.to_string());
    session.status = SessionStatus::Running;
    session.session_type = session_type;
    write_test_pid_identity(work_dir, &session, std::process::id()).unwrap();
    save_session(&session, work_dir).unwrap();
    session
}

/// Mark `session` finished on disk, so it no longer counts as live.
pub(crate) fn finish_session(work_dir: &Path, session: &mut Session) {
    session.status = SessionStatus::Completed;
    save_session(session, work_dir).unwrap();
}
