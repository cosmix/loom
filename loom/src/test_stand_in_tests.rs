//! The stand-in helper's own contract: cleanup removes every shape it
//! spawns, a refused or ineffective kill fails within the bound without
//! blocking, an id that may have been reused is never signalled, and the
//! guards skip or fail as the strict-mode switch says.

use std::io::{BufRead, BufReader};
use std::process::Stdio;
use std::sync::mpsc;

use super::adopted::retire_orphan;
use super::*;
use crate::process::sandbox_probe::{orphans_signalable, skip_without_process_tree, spawn_orphan};
use crate::process::{is_process_alive, process_is_zombie, ProcessIdentity};

const SHORT: Duration = Duration::from_millis(200);

/// A sender that fails the test if cleanup signals anything.
fn never(_: Reach, _: Pid) -> nix::Result<()> {
    panic!("cleanup signalled an id it no longer owns")
}

/// Wait up to five seconds for `path` to hold a pid.
fn pid_in(path: &std::path::Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let read = std::fs::read_to_string(path).ok();
        if let Some(pid) = read.and_then(|text| text.trim().parse().ok()) {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "{} never got a pid",
            path.display()
        );
        std::thread::sleep(POLL);
    }
}

/// `cat` on a pipe in a process group of its own: it ends when its stdin
/// closes, so a test that refuses its kill can still remove it.
fn stdin_bound_group() -> Child {
    Command::new("/bin/cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("spawn cat")
}

fn end_by_eof(mut child: Child) {
    drop(child.stdin.take());
    let ended = wait_with_deadline(&mut child, Duration::from_secs(5));
    assert!(
        matches!(ended, Ok(Some(_))),
        "cat ends on end of input: {ended:?}"
    );
}

/// Run `retire` on another thread and fail if it does not return in time.
fn retire_off_thread(
    child: Child,
    send: fn(Reach, Pid) -> nix::Result<()>,
) -> (Result<(), String>, Duration, Child) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut child = child;
        let started = Instant::now();
        let outcome = retire(&mut child, Reach::Group, send, SHORT);
        tx.send((outcome, started.elapsed(), child)).unwrap();
    });
    rx.recv_timeout(SHORT + Duration::from_secs(5))
        .expect("retire blocked past its bound")
}

#[test]
fn a_supervisor_tree_and_its_busy_disguised_grandchild_are_gone_after_drop() {
    require_signals!();
    let dir = tempfile::tempdir().unwrap();
    let worker = dir.path().join("worker.pid");
    let body = "bash -c \"while :; do :; done\" & echo $! > \"$1\"; wait";
    let script = format!("(exec -a python3 bash -c '{body}' supervisor \"$1\"); :");
    let mut command = Command::new("bash");
    command.args(["-c", &script, "launcher"]).arg(&worker);
    let tree = StandIn::group_leader(&mut command);
    let leader = tree.id();
    let grandchild = pid_in(&worker);
    assert!(is_process_alive(grandchild));

    drop(tree);

    assert!(!is_process_alive(leader), "launcher {leader} survived");
    assert!(
        !is_process_alive(grandchild),
        "worker {grandchild} survived"
    );
}

#[test]
fn a_direct_child_is_killed_and_reaped_on_drop() {
    require_signals!();
    let child = StandIn::direct(Command::new("sleep").arg("30"));
    let pid = child.id();
    drop(child);
    assert!(!is_process_alive(pid), "sleep {pid} survived");
}

#[test]
fn a_child_that_already_exited_needs_no_kill() {
    require_signals!();
    let mut child = StandIn::direct(&mut Command::new("true"));
    assert!(child.wait_within(Duration::from_secs(5)).success());
    assert_eq!(retire(child.child(), Reach::Child, never, SHORT), Ok(()));
}

#[test]
fn a_child_meant_to_end_alone_that_still_runs_at_cleanup_is_killed_within_the_bound() {
    require_signals!();
    let child = StandIn::exits_alone(Command::new("sleep").arg("30"));
    let pid = child.id();
    let started = Instant::now();
    drop(child);
    let took = started.elapsed();
    assert!(took < CLEANUP_BOUND, "cleanup took {took:?}");
    assert!(!is_process_alive(pid), "sleep {pid} survived");
}

/// A leader that exited on its own leaves a group of one zombie, whose kill
/// macOS refuses with EPERM; cleanup reaps it all the same.
#[test]
fn a_group_leader_that_exited_unreaped_is_reaped_on_drop() {
    require_orphan_signals!();
    let leader = StandIn::group_leader(&mut Command::new("true"));
    let pid = leader.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !process_is_zombie(pid) {
        assert!(Instant::now() < deadline, "true {pid} never exited");
        std::thread::sleep(POLL);
    }
    drop(leader);
    assert!(
        !process_is_zombie(pid),
        "the leader {pid} was left unreaped"
    );
}

/// `unreaped` reads a zombie without reaping it, so the test's own reap
/// still gets the exit status.
#[test]
fn an_exited_unreaped_child_reads_unreaped_until_the_test_reaps_it() {
    if skip_without_process_tree(&test_name()) {
        return;
    }
    let mut child = StandIn::exits_alone(Command::new("sh").args(["-c", "exit 7"]));
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !process_is_zombie(pid) {
        assert!(Instant::now() < deadline, "sh {pid} never exited");
        std::thread::sleep(POLL);
    }
    assert_eq!(unreaped(pid), Ok(true));
    assert_eq!(unreaped(pid), Ok(true));
    let status = child.child().try_wait().expect("reap the zombie");
    assert_eq!(status.and_then(|status| status.code()), Some(7));
    assert_eq!(unreaped(pid), Ok(false));
}

#[test]
fn a_reaped_leader_whose_group_is_gone_is_not_signalled() {
    require_signals!();
    let mut leader = StandIn::group_leader(&mut Command::new("true"));
    assert!(leader.wait_within(Duration::from_secs(5)).success());
    assert_eq!(retire(leader.child(), Reach::Group, never, SHORT), Ok(()));
}

/// A leader the test reaped no longer pins its group id, so a group still
/// answering it is reported and left unsignalled.
#[test]
fn a_group_that_outlives_its_reaped_leader_is_reported_not_signalled() {
    require_signals!();
    // A background job's stdin is /dev/null unless redirected, hence fd 3.
    let script = "exec 3<&0; cat <&3 >/dev/null 3<&- & echo $!";
    let mut command = Command::new("sh");
    command
        .args(["-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut leader = StandIn::group_leader(&mut command);
    let mut line = String::new();
    let stdout = leader.child().stdout.take().expect("stdout");
    BufReader::new(stdout).read_line(&mut line).unwrap();
    assert!(leader.wait_within(Duration::from_secs(5)).success());

    let why = retire(leader.child(), Reach::Group, never, SHORT).expect_err("a survivor");
    assert!(why.contains("outlived"), "{why}");

    // `cat` holds the leader's stdin and ends once it closes.
    drop(leader.child().stdin.take());
    let member: u32 = line.trim().parse().expect("the member's pid");
    let deadline = Instant::now() + Duration::from_secs(5);
    let gone = await_gone(member, "process", deadline, Duration::from_secs(5), || {
        !is_process_alive(member)
    });
    assert_eq!(gone, Ok(()));
}

#[test]
fn waiting_past_the_bound_fails_the_test_and_still_cleans_up() {
    require_signals!();
    let mut pid = None;
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut child = StandIn::direct(Command::new("sleep").arg("30"));
        pid = Some(child.id());
        child.wait_within(Duration::from_millis(50))
    }));
    let panic = outcome.expect_err("a stand-in outliving the bound fails the test");
    let text = panic.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(text.contains("still running"), "{text}");
    let pid = pid.expect("the stand-in started");
    assert!(!is_process_alive(pid), "sleep {pid} survived the unwinding");
}

#[test]
fn an_orphan_is_killed_on_drop() {
    require_orphan_signals!();
    let orphan = orphan();
    let pid = orphan.pid();
    assert!(is_process_alive(pid));
    drop(orphan);
    assert!(!is_process_alive(pid), "orphan {pid} survived");
}

#[test]
fn an_orphan_with_another_start_time_is_not_signalled() {
    require_orphan_signals!();
    if skip_without_process_tree(&test_name()) {
        return;
    }
    let orphan = orphan();
    let identity = orphan.identity.expect("the orphan was alive at adoption");
    let recorded = identity.start_time.expect("a visible start time");
    let stranger = ProcessIdentity {
        pid: orphan.pid(),
        start_time: Some(recorded + 1),
    };
    let never = |_: Pid| -> nix::Result<()> { panic!("a reused pid was signalled") };
    assert_eq!(retire_orphan(stranger, never, SHORT), Ok(()));
}

/// A pid adopted after its process ended may come to name another process.
#[test]
fn an_orphan_adopted_after_it_ended_is_never_signalled() {
    let pid = spawn_orphan("exit 0", &[]).expect("spawn an orphan that exits");
    let deadline = Instant::now() + Duration::from_secs(5);
    let gone = await_gone(pid, "process", deadline, Duration::from_secs(5), || {
        !is_process_alive(pid)
    });
    assert_eq!(gone, Ok(()));
    assert!(Orphan::adopt(pid).identity.is_none());
}

#[test]
fn an_orphan_without_a_start_time_is_reported_not_signalled() {
    require_orphan_signals!();
    let orphan = orphan();
    let unknown = ProcessIdentity {
        pid: orphan.pid(),
        start_time: None,
    };
    let never = |_: Pid| -> nix::Result<()> { panic!("an unverified pid was signalled") };
    let why = retire_orphan(unknown, never, SHORT).expect_err("an unverifiable orphan");
    assert!(
        why.contains(&orphan.pid().to_string()) && why.contains("no start time"),
        "{why}"
    );
}

#[test]
fn a_group_whose_leader_was_gone_at_adoption_is_never_signalled() {
    assert!(AdoptedGroup::adopt(reaped_pid()).leader.is_none());
}

#[test]
fn an_adopted_group_without_a_start_time_is_reported_not_signalled() {
    let child = stdin_bound_group();
    let pid = child.id();
    let group = AdoptedGroup {
        leader: Some(ProcessIdentity {
            pid,
            start_time: None,
        }),
    };
    let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(group)));
    let panic = dropped.expect_err("an unverifiable group fails the test");
    let why = panic.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(
        why.contains(&pid.to_string()) && why.contains("no start time"),
        "{why}"
    );
    assert!(
        is_process_alive(pid),
        "the unverified group {pid} was signalled"
    );
    end_by_eof(child);
}

#[test]
fn a_refused_group_kill_fails_at_once_naming_the_pid_and_the_error() {
    let child = stdin_bound_group();
    let pid = child.id();
    let (outcome, elapsed, child) = retire_off_thread(child, |_, _| Err(Errno::EPERM));
    let why = outcome.expect_err("a refused kill must fail the test");
    assert!(
        why.contains(&pid.to_string()) && why.contains("EPERM"),
        "{why}"
    );
    assert!(elapsed < SHORT, "a refused kill waited {elapsed:?}");
    end_by_eof(child);
}

#[test]
fn an_ineffective_group_kill_fails_once_the_bound_passes() {
    let child = stdin_bound_group();
    let pid = child.id();
    let (outcome, elapsed, child) = retire_off_thread(child, |_, _| Ok(()));
    let why = outcome.expect_err("a survivor must fail the test");
    assert!(
        why.contains(&pid.to_string()) && why.contains("still running"),
        "{why}"
    );
    assert!(elapsed >= SHORT, "gave up after {elapsed:?}");
    end_by_eof(child);
}

#[test]
fn a_refused_orphan_kill_fails_at_once() {
    require_orphan_signals!();
    let orphan = orphan();
    // The bound is long, so returning well inside it shows the refusal did not wait.
    let bound = Duration::from_secs(30);
    let started = Instant::now();
    let identity = orphan.identity.expect("the orphan was alive at adoption");
    let outcome = retire_orphan(identity, |_| Err(Errno::EPERM), bound);
    let why = outcome.expect_err("a refused kill must fail the test");
    assert!(why.contains(&orphan.pid().to_string()), "{why}");
    assert!(started.elapsed() < bound / 6, "a refused kill waited");
}

#[test]
fn a_cleanup_failure_panics_unless_the_test_is_already_failing() {
    let alone = std::panic::catch_unwind(|| settle(Err("stand-in 7: refused".into())));
    let panic = alone.expect_err("a cleanup failure must fail the test");
    assert_eq!(
        panic.downcast_ref::<String>().map(String::as_str),
        Some("stand-in 7: refused")
    );

    struct FailsOnDrop;
    impl Drop for FailsOnDrop {
        fn drop(&mut self) {
            settle(Err("stand-in 8: refused".into()));
        }
    }
    let unwinding = std::panic::catch_unwind(|| {
        let _cleanup = FailsOnDrop;
        panic!("the test's own failure");
    });
    let panic = unwinding.expect_err("the test's own panic propagates");
    assert_eq!(
        panic.downcast_ref::<&str>(),
        Some(&"the test's own failure")
    );
}

/// The guards skip exactly when their probes fail (and panic instead under
/// `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`, which `skip_unless`'s own tests cover).
#[test]
fn the_guards_follow_their_probes() {
    let strict = std::env::var("LOOM_TEST_REQUIRE_SANDBOX_FREE").as_deref() == Ok("1");
    if strict {
        assert!(!skip_without_signals() && !skip_without_orphans());
        return;
    }
    assert_eq!(skip_without_signals(), !process_groups_signalable());
    let orphans = process_groups_signalable() && orphans_signalable();
    assert_eq!(skip_without_orphans(), !orphans);
}
