//! Real CLI and daemon admission paths sharing a controlled backend.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use super::super::resolver_attempts::ReservedAttempt;
use super::test_fixtures::{orchestrator_with_conflict, repo_with_conflicting_stage_branches};
use crate::commands::stage::{test_spawn_merge_resolver, MergeResolverResult};
use crate::orchestrator::signals::{find_live_merge_session_for_stage, merge_test_backend};
use crate::verify::transitions::load_stage;

struct ChildGuard(Arc<Mutex<Option<std::process::Child>>>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn install_backend(
    work: &std::path::Path,
    fail_publication: bool,
) -> (
    mpsc::Sender<()>,
    mpsc::Receiver<String>,
    Arc<AtomicUsize>,
    ChildGuard,
) {
    let (entered, ready) = mpsc::channel();
    let (release, proceed) = mpsc::channel();
    let proceed = Mutex::new(proceed);
    let count = Arc::new(AtomicUsize::new(0));
    let child = Arc::new(Mutex::new(None::<std::process::Child>));
    let calls = count.clone();
    let child_slot = child.clone();
    let child_work = work.to_owned();
    merge_test_backend::install(
        work,
        Arc::new(move |mut session| {
            calls.fetch_add(1, Ordering::SeqCst);
            entered.send(session.id.clone()).unwrap();
            proceed.lock().unwrap().recv().unwrap();
            let process = std::process::Command::new("sleep").arg("60").spawn()?;
            let pid = process.id();
            *child_slot.lock().unwrap() = Some(process);
            session.pid = Some(pid);
            let start = crate::process::process_start_time(pid).unwrap();
            std::fs::create_dir_all(child_work.join("pids"))?;
            std::fs::write(
                child_work
                    .join("pids")
                    .join(format!("{}-{}.pid", session.tracking_key, session.id)),
                format!("{pid}\n{start}\n"),
            )?;
            if fail_publication {
                session.stage_id = Some("wrong-stage".into());
            }
            Ok(session)
        }),
    );
    (release, ready, count, ChildGuard(child))
}

fn run_competition(
    repo: &std::path::Path,
    mut daemon: crate::orchestrator::core::Orchestrator,
    release: mpsc::Sender<()>,
    ready: mpsc::Receiver<String>,
    count: &AtomicUsize,
    fail_publication: bool,
) -> String {
    let work = daemon.config.work_dir.clone();
    let stage = load_stage("race", &work).unwrap();
    std::thread::scope(|scope| {
        let cli_stage = stage.clone();
        let cli_work = work.clone();
        let root = repo.to_owned();
        let cli = scope
            .spawn(move || test_spawn_merge_resolver(&cli_stage, &[], "main", &root, &cli_work));
        let id = ready.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(work.join("signals").join(format!("{id}.md")).exists());
        let attempt = ReservedAttempt::record(&work, "race", 0).unwrap();
        let daemon_stage = stage.clone();
        let (done, result) = mpsc::channel();
        let (starting, started) = mpsc::channel();
        scope.spawn(move || {
            starting.send(()).unwrap();
            done.send(daemon.spawn_merge_resolution_session(&daemon_stage, attempt))
                .unwrap()
        });
        started.recv().unwrap();
        assert!(result.recv_timeout(Duration::from_millis(100)).is_err());
        release.send(()).unwrap();
        let cli_result = cli.join().unwrap();
        assert_cli_result(cli_result, fail_publication, &work, &id);
        assert!(!result
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(
            find_live_merge_session_for_stage("race", &work).unwrap(),
            Some(id.clone())
        );
        assert!(work.join("signals").join(format!("{id}.md")).exists());
        id
    })
}

fn assert_cli_result(
    result: anyhow::Result<MergeResolverResult>,
    failure: bool,
    work: &std::path::Path,
    id: &str,
) {
    if failure {
        assert!(result.is_err());
        let saved = crate::fs::session_files::load_session_exact(work, id)
            .unwrap()
            .unwrap();
        assert_eq!(
            saved.status,
            crate::models::session::SessionStatus::Spawning
        );
    } else {
        assert!(matches!(result.unwrap(), MergeResolverResult::Spawned(ref found) if found == id));
    }
}

fn competing_launches(fail_publication: bool) {
    let repo = repo_with_conflicting_stage_branches(&["race"]);
    let daemon = orchestrator_with_conflict(repo.path(), "race");
    let work = daemon.config.work_dir.clone();
    let (release, ready, count, guard) = install_backend(&work, fail_publication);
    let id = run_competition(
        repo.path(),
        daemon,
        release,
        ready,
        &count,
        fail_publication,
    );
    merge_test_backend::remove(&work);
    let mut process = guard.0.lock().unwrap().take().unwrap();
    process.kill().unwrap();
    process.wait().unwrap();
    crate::fs::session_files::mark_session_terminal_reason(
        &work,
        &id,
        crate::models::session::SessionStatus::Crashed,
        crate::models::session::SessionExitReason::Crashed,
    )
    .unwrap();
    assert_eq!(
        find_live_merge_session_for_stage("race", &work).unwrap(),
        None
    );
}

#[test]
fn cli_and_daemon_launch_only_one_resolver() {
    competing_launches(false);
}

#[test]
fn failed_publication_after_spawn_blocks_the_other_surface() {
    competing_launches(true);
}
