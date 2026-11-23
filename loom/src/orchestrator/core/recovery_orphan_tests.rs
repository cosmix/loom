//! Restart recovery of current records with no process: never-launched and
//! young `Spawning` records, dead waiters, and a retry write that fails.

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, SystemTime};

use serial_test::serial;
use tempfile::TempDir;

use super::{minimal_stage_definition, restart_session, restart_stage, resume_orchestrator};
use crate::fs::session_files::save_session;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::orchestrator::core::recovery::Recovery;
use crate::orchestrator::core::Orchestrator;
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::terminal::native::{wrapper_script_path, NativeBackend};
use crate::plan::ExecutionGraph;
use crate::verify::transitions::{load_stage, save_stage, update_stage};

/// An orchestrator whose graph has `alpha` executing and whose backend finds
/// no process.
fn dead_backend_orchestrator(temp: &TempDir) -> Orchestrator {
    let mut graph = ExecutionGraph::build(vec![minimal_stage_definition("alpha")]).unwrap();
    graph.mark_queued("alpha").unwrap();
    graph.mark_executing("alpha").unwrap();
    let mut orchestrator = resume_orchestrator(temp, graph);
    orchestrator.liveness = LivenessService::fixed_for_tests(false);
    orchestrator
}

fn write_wrapper(work: &std::path::Path, session: &Session) -> std::path::PathBuf {
    let (_, pid_key) = NativeBackend::window_title_and_pid_key(session).unwrap();
    let wrapper = wrapper_script_path(work, &pid_key);
    std::fs::create_dir_all(wrapper.parent().unwrap()).unwrap();
    std::fs::write(&wrapper, "").unwrap();
    wrapper
}

#[test]
fn a_never_launched_record_is_retired_at_startup() {
    let temp = TempDir::new().unwrap();
    let mut orchestrator = dead_backend_orchestrator(&temp);
    let work = orchestrator.config.work_dir.clone();
    let session = restart_session("alpha");
    save_session(&session, &work).unwrap();
    save_stage(&restart_stage("alpha", &session.id), &work).unwrap();

    orchestrator.recover_current_sessions().unwrap();

    let stage = load_stage("alpha", &work).unwrap();
    assert_ne!(stage.status, StageStatus::Executing);
    assert_eq!(stage.session, None);
    assert!(orchestrator.deferred_orphans.is_empty());
}

#[test]
#[serial]
fn a_young_launched_record_is_retired_on_a_later_pass() {
    let temp = TempDir::new().unwrap();
    let mut orchestrator = dead_backend_orchestrator(&temp);
    let work = orchestrator.config.work_dir.clone();
    let mut session = restart_session("alpha");
    save_session(&session, &work).unwrap();
    save_stage(&restart_stage("alpha", &session.id), &work).unwrap();
    let wrapper = write_wrapper(&work, &session);

    orchestrator.recover_current_sessions().unwrap();

    let young = load_stage("alpha", &work).unwrap();
    assert_eq!(young.status, StageStatus::Executing);
    assert_eq!(
        orchestrator.deferred_orphans.get("alpha"),
        Some(&session.id)
    );

    session.created_at -= chrono::Duration::seconds(120);
    save_session(&session, &work).unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&wrapper)
        .unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(120))
        .unwrap();
    orchestrator.begin_scheduling_pass();

    let retired = load_stage("alpha", &work).unwrap();
    assert_ne!(retired.status, StageStatus::Executing);
    assert_eq!(retired.session, None);
    assert!(orchestrator.deferred_orphans.is_empty());
}

#[test]
fn a_dead_waiter_is_recovered_at_startup() {
    let temp = TempDir::new().unwrap();
    let mut orchestrator = dead_backend_orchestrator(&temp);
    let work = orchestrator.config.work_dir.clone();
    let mut session = restart_session("alpha");
    session.created_at -= chrono::Duration::minutes(10);
    save_session(&session, &work).unwrap();
    let mut stage = restart_stage("alpha", &session.id);
    stage.status = StageStatus::WaitingForInput;
    save_stage(&stage, &work).unwrap();

    orchestrator.recover_current_sessions().unwrap();

    let after = load_stage("alpha", &work).unwrap();
    assert_ne!(after.status, StageStatus::WaitingForInput);
    assert_eq!(after.session, None);
}

/// Restores the stages directory's mode on drop, panics included, so the
/// temporary directory can be removed.
struct ReadOnlyDir(std::path::PathBuf);

impl ReadOnlyDir {
    fn set(path: std::path::PathBuf) -> Self {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o555)).unwrap();
        Self(path)
    }
}

impl Drop for ReadOnlyDir {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn a_failed_retry_write_does_not_end_the_pass() {
    // SAFETY: geteuid has no arguments and only reads process credentials.
    if unsafe { libc::geteuid() } == 0 {
        return; // root writes into a 0o555 directory, so the write cannot fail
    }
    let temp = TempDir::new().unwrap();
    let mut orchestrator = dead_backend_orchestrator(&temp);
    let work = orchestrator.config.work_dir.clone();
    let mut stage = restart_stage("alpha", "session-gone");
    stage.status = StageStatus::Blocked;
    stage.session = None;
    save_stage(&stage, &work).unwrap();
    let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
    update_stage("alpha", &work, |current| {
        current.failure_info = Some(FailureInfo {
            failure_type: FailureType::SessionCrash,
            detected_at: hour_ago,
            evidence: Vec::new(),
        });
        current.retry_count = 0;
        current.last_failure_at = Some(hour_ago);
        Ok(())
    })
    .unwrap();

    let read_only = ReadOnlyDir::set(work.join("stages"));
    let result = orchestrator.sync_graph_with_stage_files();
    let status = load_stage("alpha", &work).map(|stage| stage.status);
    drop(read_only);

    result.expect("one unwritable retry must not end the scheduling pass");
    assert_eq!(status.unwrap(), StageStatus::Blocked);
}
