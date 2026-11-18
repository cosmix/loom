//! Authenticating a direct worker against live process trees, with a same-named,
//! same-environment impostor, at the clock points the hold is bounded by.

use std::os::unix::process::CommandExt;
use std::process::{Child, Command};
use std::time::Instant;

use chrono::TimeDelta;

use super::*;
use crate::fs::save_session;
use crate::models::session::{Session, SessionStatus};

const INVOCATION: &str = "inv-0123456789abcdef0123456789abcdef";

pub(crate) struct LiveTree(Child);

impl Drop for LiveTree {
    fn drop(&mut self) {
        // SAFETY: the group was created for this child alone by process_group(0).
        unsafe {
            libc::killpg(self.0.id() as i32, libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}

/// A process whose argv reads as the installed supervisor, running `body`.
fn supervisor_shell(body: &str) -> String {
    format!("(exec -a python3 bash -c '{body}' /hooks/{SUPERVISOR_SCRIPT})")
}

fn spawn_group(script: String) -> LiveTree {
    let child = Command::new("bash")
        .arg("-c")
        .arg(script)
        .process_group(0)
        .spawn()
        .unwrap();
    LiveTree(child)
}

/// A launcher running a supervisor-named process running a CPU-burning worker.
/// The trailing `:` keeps the launcher shell from exec-ing its last command.
pub(crate) fn spawn_tree(quoted: &str) -> LiveTree {
    let supervisor = supervisor_shell("bash -c \"while :; do :; done\" & wait");
    spawn_group(format!("{supervisor}; : '{quoted}'"))
}

/// A launcher running the wrapper's bounded wait on one companion job, returned
/// once its process tree reads that way.
pub(crate) fn spawn_companion_wait(invocation: &str, job_id: &str) -> LiveTree {
    let tree = spawn_group(format!(
        "(exec -a node bash -c 'sleep 60; :' /hooks/{COMPANION_SCRIPT} status {job_id} --wait); \
         : '--invocation-id {invocation}'"
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    let agent = std::process::id();
    while !awaits_job(agent, &process_rows(), invocation, job_id, process_argv) {
        assert!(Instant::now() < deadline, "companion wait never appeared");
        std::thread::sleep(Duration::from_millis(20));
    }
    tree
}

fn live_supervisor(invocation: &str) -> Option<u32> {
    authorized_supervisor(
        std::process::id(),
        &process_rows(),
        invocation,
        process_argv,
    )
}

pub(crate) fn wait_for_supervisor(invocation: &str) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pid) = live_supervisor(invocation) {
            return pid;
        }
        assert!(Instant::now() < deadline, "supervisor never appeared");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn nonce(tag: u64) -> String {
    format!("inv-{:016x}{tag:016x}", u64::from(std::process::id()))
}

#[test]
fn a_same_named_same_environment_impostor_gains_nothing_from_a_live_tree() {
    let authentic = nonce(1);
    let stolen = nonce(2);
    let unused = nonce(3);
    let _real = spawn_tree(&format!("--invocation-id {authentic}"));
    let supervisor = wait_for_supervisor(&authentic);

    // Same executable names, same environment and ancestry shape, no id.
    let _nameless = spawn_tree("--invocation-id inv-not-minted");
    assert_eq!(live_supervisor(&unused), None);

    // Reads a minted id but cannot get it into a launcher's argv: the guard
    // refuses the literal, so the launcher's shell computes it after exec. Its
    // descendants carry the id; the direct child's kernel-visible argv cannot.
    let script = format!(
        "id=$(printf %s {} {}); (exec -a python3 bash -c 'bash -c \"while :; do :; done\" & wait' \
         /hooks/{SUPERVISOR_SCRIPT} \"--invocation-id $id\"); :",
        &stolen[..10],
        &stolen[10..],
    );
    assert!(!script.contains(&stolen));
    let _computed = spawn_group(script);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(live_supervisor(&stolen), None);

    // The authorized launch is still recognized among them. A second launcher
    // that does quote it makes the evidence ambiguous, so it holds nothing.
    assert_eq!(live_supervisor(&authentic), Some(supervisor));
    let _quoting = spawn_tree(&format!("--invocation-id {authentic}"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while live_supervisor(&authentic).is_some() {
        assert!(Instant::now() < deadline, "ambiguity was never refused");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn wait_for_worker(supervisor: u32) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while worker_cpu_ticks(supervisor, &process_rows(), process_cpu_ticks).is_none() {
        assert!(Instant::now() < deadline, "worker never started");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub(crate) fn wait_until_gone(invocation: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while live_supervisor(invocation).is_some() {
        assert!(Instant::now() < deadline, "tree never exited");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Bind `session` to this test process, as the wrapper's PID file would.
pub(crate) fn mark_own_agent(work: &Path, session: &Session) {
    save_session(session, work).unwrap();
    let (_, key) = NativeBackend::window_title_and_pid_key(session).unwrap();
    let pid = std::process::id();
    let path = work.join("pids").join(format!("{key}.pid"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let start = process_start_time(pid).unwrap();
    std::fs::write(path, format!("{pid}\n{start}\n")).unwrap();
}

fn live_session(work: &Path) -> Session {
    let mut session = Session::new();
    session.assign_to_stage("stage".to_string());
    session.status = SessionStatus::Running;
    mark_own_agent(work, &session);
    session
}

#[test]
fn hold_is_bounded_and_ends_with_the_worker_at_the_same_clock_points() {
    let work = tempfile::TempDir::new().unwrap();
    let session = live_session(work.path());
    let invocation = nonce(4);
    let budget = Duration::from_secs(300);
    let start = Utc::now();
    let holds = |at: i64| {
        direct_worker_holds(
            work.path(),
            &session.id,
            &invocation,
            start,
            budget,
            start + TimeDelta::seconds(at),
        )
    };
    // No launch yet: authorization alone holds nothing, at any clock point.
    assert!(!holds(1));

    let tree = spawn_tree(&format!("--invocation-id {invocation}"));
    let supervisor = wait_for_supervisor(&invocation);
    let deadline = Instant::now() + Duration::from_secs(10);
    while worker_cpu_ticks(supervisor, &process_rows(), process_cpu_ticks).unwrap_or(0) == 0 {
        assert!(Instant::now() < deadline, "worker never accrued CPU");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(holds(1), "first sample of a live worker");
    let burn_a_tick = || std::thread::sleep(Duration::from_millis(100));
    for at in [300, 600, 900, 1200, 1559] {
        burn_a_tick();
        assert!(holds(at), "a growing worker holds at {at}");
    }
    burn_a_tick();
    assert!(!holds(1560), "the wall window ends the hold however busy");

    // A zero budget disables holds, as it does for every stall line.
    assert!(!direct_worker_holds(
        work.path(),
        &session.id,
        &invocation,
        start,
        Duration::ZERO,
        start + TimeDelta::seconds(1),
    ));

    // At a clock point a live worker held, a dead one does not.
    drop(tree);
    wait_until_gone(&invocation);
    assert!(!holds(900), "dead worker");
}

#[test]
fn a_worker_with_no_growth_expires_after_one_idle_budget() {
    let work = tempfile::TempDir::new().unwrap();
    let session = live_session(work.path());
    let invocation = nonce(5);
    let _tree = spawn_group(format!(
        "{}; : '--invocation-id {invocation}'",
        supervisor_shell("sleep 60 & wait")
    ));
    let supervisor = wait_for_supervisor(&invocation);
    wait_for_worker(supervisor);
    // Let the worker finish exec so startup ticks cannot read as growth.
    std::thread::sleep(Duration::from_millis(300));
    let start = Utc::now();
    let holds = |at: i64| {
        direct_worker_holds(
            work.path(),
            &session.id,
            &invocation,
            start,
            Duration::from_secs(300),
            start + TimeDelta::seconds(at),
        )
    };
    assert!(holds(10), "first sample");
    assert!(holds(309), "inside the idle budget");
    assert!(!holds(310), "an open tool alone earns no more");
}

#[test]
fn a_session_that_ended_or_is_missing_holds_nothing() {
    let work = tempfile::TempDir::new().unwrap();
    let now = Utc::now();
    let budget = Duration::from_secs(300);
    assert!(!direct_worker_holds(
        work.path(),
        "missing-session",
        INVOCATION,
        now,
        budget,
        now
    ));
    let mut session = live_session(work.path());
    session.status = SessionStatus::Completed;
    save_session(&session, work.path()).unwrap();
    assert!(!direct_worker_holds(
        work.path(),
        &session.id,
        INVOCATION,
        now,
        budget,
        now
    ));
}
