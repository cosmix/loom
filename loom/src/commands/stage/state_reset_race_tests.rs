//! Reset and scheduler assignment must serialize across liveness scanning.

use super::{loop_recovery, LiveAgent};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::fs::work_dir::write_terminal_config;
use crate::models::session::{
    Session, SessionBackendKind, SessionExitReason, SessionStatus, TerminalConfig,
};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, save_stage, update_stage};
use anyhow::Result;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::Mutex;
use std::thread::JoinHandle;
use tempfile::TempDir;

struct RacingAssignment {
    worker: Mutex<Option<JoinHandle<Session>>>,
}

impl RacingAssignment {
    fn new() -> Self {
        Self {
            worker: Mutex::new(None),
        }
    }

    fn finish(&self) -> Session {
        self.worker.lock().unwrap().take().unwrap().join().unwrap()
    }
}

impl loop_recovery::ResetRuntime for RacingAssignment {
    fn kill(&self, _work_dir: &Path, _agents: &[LiveAgent]) {}
    fn identity_missing(&self, _work_dir: &Path, _agent: &LiveAgent) -> bool {
        false
    }
    fn is_alive(&self, _work_dir: &Path, _agent: &LiveAgent) -> Result<bool> {
        Ok(false)
    }
    fn wait(&self) {}

    fn after_scan(&self, work_dir: &Path, stage_id: &str) {
        let path = work_dir
            .join("stage-admission")
            .join(format!("{stage_id}.lock"));
        let probe = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        // SAFETY: `probe` owns the open descriptor and remains alive throughout the call.
        let result = unsafe { libc::flock(probe.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        assert_eq!(result, -1, "reset released admission during its scan");

        let work = work_dir.to_path_buf();
        let stage_id = stage_id.to_string();
        let worker = std::thread::spawn(move || {
            let _admission = crate::fs::stage_admission::lock(&work, &stage_id).unwrap();
            let mut session = Session::new();
            session.id = "new-owner".to_string();
            session.assign_to_stage(stage_id.clone());
            session.status = SessionStatus::Running;
            save_session(&session, &work).unwrap();
            update_stage(&stage_id, &work, |stage| {
                stage.status = StageStatus::Executing;
                stage.begin_attempt(chrono::Utc::now());
                stage.session = Some(session.id.clone());
                Ok(())
            })
            .unwrap();
            session
        });
        *self.worker.lock().unwrap() = Some(worker);
    }
}

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

fn stage(work: &Path, status: StageStatus, session: Option<&Session>) {
    let mut stage = Stage::new("alpha".to_string(), None);
    stage.id = "alpha".to_string();
    stage.status = status;
    stage.session = session.map(|item| item.id.clone());
    save_stage(&stage, work).unwrap();
}

fn assert_new_assignment_survived(work: &Path, runtime: &RacingAssignment) {
    let session = runtime.finish();
    let stage = load_stage("alpha", work).unwrap();
    assert_eq!(stage.status, StageStatus::Executing);
    assert_eq!(stage.session.as_deref(), Some(session.id.as_str()));
    assert!(stage.attempt_started_at.is_some());
    assert_eq!(
        load_session_exact(work, &session.id)
            .unwrap()
            .unwrap()
            .status,
        SessionStatus::Running
    );
}

#[test]
fn reset_scan_serializes_a_new_scheduler_assignment() {
    let temp = work_dir();
    stage(temp.path(), StageStatus::Queued, None);
    let runtime = RacingAssignment::new();

    loop_recovery::reset_with(temp.path(), "alpha", false, false, &runtime).unwrap();

    assert_new_assignment_survived(temp.path(), &runtime);
}

#[test]
fn reset_kill_wait_serializes_a_new_scheduler_assignment() {
    let temp = work_dir();
    let mut old = Session::new();
    old.assign_to_stage("alpha".to_string());
    old.status = SessionStatus::Running;
    save_session(&old, temp.path()).unwrap();
    stage(temp.path(), StageStatus::Executing, Some(&old));
    let runtime = RacingAssignment::new();

    loop_recovery::reset_with(temp.path(), "alpha", false, true, &runtime).unwrap();

    assert_new_assignment_survived(temp.path(), &runtime);
    let retired = load_session_exact(temp.path(), &old.id).unwrap().unwrap();
    assert_eq!(retired.exit_reason, Some(SessionExitReason::OperatorStop));
}
