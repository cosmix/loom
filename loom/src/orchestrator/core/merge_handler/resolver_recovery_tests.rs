//! A resolver the daemon launched outlives both a failed publication and a
//! daemon restart, and a launch that died gives its stage back.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};

use super::super::resolver_spawn::test_fixtures::{
    orchestrator_with_conflict, repo_with_conflicting_stage_branches, resolver_for, write_pid_file,
    write_signal,
};
use crate::fs::session_files::{load_session_exact, save_session};
use crate::models::session::{SessionBackendKind, SessionStatus};
use crate::models::stage::StageStatus;
use crate::orchestrator::core::merge_handler::resolver_attempts::merge_resolver_attempts;
use crate::orchestrator::signals::merge_test_backend;
use crate::verify::transitions::load_stage;

const STAGE: &str = "pub";

/// [`orchestrator_with_conflict`] whose graph knows the stage, as a snapshot
/// of the stage files requires.
fn orchestrator_over(root: &std::path::Path) -> crate::orchestrator::core::Orchestrator {
    let mut orchestrator = orchestrator_with_conflict(root, STAGE);
    orchestrator.graph =
        crate::plan::ExecutionGraph::build(vec![crate::plan::schema::StageDefinition {
            id: STAGE.to_string(),
            name: STAGE.to_string(),
            working_dir: ".".to_string(),
            ..Default::default()
        }])
        .unwrap();
    orchestrator
}

struct KillOnDrop(Arc<Mutex<Option<std::process::Child>>>);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// The publication after a real process started fails: the session record is
/// unreadable by then, as it is for a disk that filled up.
#[test]
fn a_failed_publication_keeps_the_resolver_tracked_and_counted() {
    let repo = repo_with_conflicting_stage_branches(&[STAGE]);
    let mut orchestrator = orchestrator_with_conflict(repo.path(), STAGE);
    let work = orchestrator.config.work_dir.clone();
    let spawns = Arc::new(AtomicUsize::new(0));
    let child = Arc::new(Mutex::new(None));
    let _guard = KillOnDrop(child.clone());
    let (calls, slot, hook_work) = (spawns.clone(), child, work.clone());
    merge_test_backend::install(
        &work,
        Arc::new(move |mut session| {
            calls.fetch_add(1, Ordering::SeqCst);
            let process = std::process::Command::new("sleep").arg("60").spawn()?;
            let pid = process.id();
            *slot.lock().unwrap() = Some(process);
            let start = crate::process::process_start_time(pid).unwrap();
            let pids = hook_work.join("pids");
            std::fs::create_dir_all(&pids)?;
            let name = format!("{}-{}.pid", session.tracking_key, session.id);
            std::fs::write(pids.join(name), format!("{pid}\n{start}\n"))?;
            let record = hook_work
                .join("sessions")
                .join(format!("{}.md", session.id));
            std::fs::write(record, "not a session record")?;
            session.pid = Some(pid);
            session.status = SessionStatus::Running;
            Ok(session)
        }),
    );

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 1);

    let tracked = orchestrator.active_sessions.get(STAGE).expect("tracked");
    assert_eq!(tracked.status, SessionStatus::Running);
    assert_eq!(merge_resolver_attempts(&work, STAGE), 1);
    let stage = load_stage(STAGE, &work).unwrap();
    assert_eq!(stage.status, StageStatus::MergeConflict);
    assert_eq!(stage.review_reason, None);

    assert_eq!(orchestrator.spawn_merge_resolution_sessions().unwrap(), 0);
    assert_eq!(spawns.load(Ordering::SeqCst), 1, "no second writer");
    merge_test_backend::remove(&work);
}

/// A restart between launch and publication: the process runs, the record
/// still says `Spawning`. Recovery publishes it and tracks it again.
#[test]
fn recovery_adopts_a_running_resolver_left_spawning() {
    let repo = repo_with_conflicting_stage_branches(&[STAGE]);
    let mut orchestrator = orchestrator_over(repo.path());
    let work = orchestrator.config.work_dir.clone();
    let mut resolver = resolver_for(STAGE);
    resolver.backend = SessionBackendKind::Tmux;
    save_session(&resolver, &work).unwrap();
    crate::orchestrator::terminal::native::write_test_pid_identity(
        &work,
        &resolver,
        std::process::id(),
    )
    .unwrap();
    let signal = write_signal(&orchestrator, &resolver.id);

    orchestrator.recover_current_sessions().unwrap();

    assert_eq!(orchestrator.active_sessions[STAGE].id, resolver.id);
    let stored = load_session_exact(&work, &resolver.id).unwrap().unwrap();
    assert_eq!(stored.status, SessionStatus::Running);
    assert_eq!(stored.pid, Some(std::process::id()));
    assert!(signal.exists());
}

/// A restart after a launch died: no process, an old reservation. Recovery
/// retires it, and nothing holds the stage.
#[test]
fn recovery_retires_a_dead_launch_reservation() {
    let repo = repo_with_conflicting_stage_branches(&[STAGE]);
    let mut orchestrator = orchestrator_over(repo.path());
    let work = orchestrator.config.work_dir.clone();
    let mut resolver = resolver_for(STAGE);
    resolver.backend = SessionBackendKind::Tmux;
    resolver.created_at = Utc::now() - Duration::seconds(300);
    save_session(&resolver, &work).unwrap();
    let signal = write_signal(&orchestrator, &resolver.id);

    orchestrator.recover_current_sessions().unwrap();

    assert!(!orchestrator.active_sessions.contains_key(STAGE));
    let stored = load_session_exact(&work, &resolver.id).unwrap().unwrap();
    assert!(stored.status.is_terminal(), "{:?}", stored.status);
    assert!(!signal.exists());
}

/// A launch reservation too young to judge is left for the next pass, and a
/// PID file proving the process dead retires it at once.
#[test]
fn recovery_leaves_a_fresh_reservation_and_retires_a_dead_process() {
    let repo = repo_with_conflicting_stage_branches(&[STAGE]);
    let mut orchestrator = orchestrator_over(repo.path());
    let work = orchestrator.config.work_dir.clone();
    let mut resolver = resolver_for(STAGE);
    resolver.backend = SessionBackendKind::Tmux;
    save_session(&resolver, &work).unwrap();
    let signal = write_signal(&orchestrator, &resolver.id);

    orchestrator.recover_current_sessions().unwrap();
    assert!(!orchestrator.active_sessions.contains_key(STAGE));
    let stored = load_session_exact(&work, &resolver.id).unwrap().unwrap();
    assert_eq!(stored.status, SessionStatus::Spawning);
    assert!(signal.exists());

    write_pid_file(&work, &resolver, Some(u64::MAX));
    orchestrator.recover_current_sessions().unwrap();
    let stored = load_session_exact(&work, &resolver.id).unwrap().unwrap();
    assert!(stored.status.is_terminal(), "{:?}", stored.status);
    assert!(!signal.exists());
}
