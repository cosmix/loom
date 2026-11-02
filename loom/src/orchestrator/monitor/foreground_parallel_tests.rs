//! CPU credit across parallel tool calls: real process trees under a stand-in
//! agent, sampled through the path the stall decision uses.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration as StdDuration, Instant};

use chrono::TimeDelta;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serde_json::json;
use serial_test::serial;

use super::*;
use crate::assets::install::test_env::InstallRootsEnv;
use crate::codex_lifecycle::test_support::{Fixture, INV_A};
use crate::orchestrator::terminal::native::NativeBackend;
use crate::process::{process_start_time, OwnedProcess};

/// A launcher running a worker that burns CPU, its pid written to `$1`, stopped
/// by the test until the test says otherwise.
const BUSY_LAUNCHER: &str =
    "sh -c 'sh -c \"while :; do :; done\" & echo $! > \"$1\"; wait' sh \"$1\" &";

/// A stand-in agent running `first` and then [`BUSY_LAUNCHER`] as tool calls.
/// `read` puts both launchers past the agent's start.
fn spawn_agent(first: &str, worker_file: &Path) -> (OwnedProcess, u32) {
    let script = format!("read line; {first} {BUSY_LAUNCHER} wait");
    let mut child = Command::new("sh")
        .args(["-c", &script, "sh"])
        .arg(worker_file)
        .stdin(Stdio::piped())
        .process_group(0)
        .spawn()
        .unwrap();
    std::thread::sleep(StdDuration::from_millis(25));
    child.stdin.take().unwrap().write_all(b"go\n").unwrap();
    let agent = OwnedProcess::new(child);
    let worker = wait_for_pid(worker_file);
    kill(Pid::from_raw(worker as i32), Signal::SIGSTOP).unwrap();
    // The launchers settle into `wait`, so only the worker can move the sum.
    std::thread::sleep(StdDuration::from_millis(50));
    (agent, worker)
}

fn wait_for_pid(path: &Path) -> u32 {
    let deadline = Instant::now() + StdDuration::from_secs(5);
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(Instant::now() < deadline, "the worker never started");
        std::thread::sleep(StdDuration::from_millis(10));
    }
}

/// Let `worker` burn CPU for a moment, then stop it again.
fn burn(worker: u32) {
    let pid = Pid::from_raw(worker as i32);
    kill(pid, Signal::SIGCONT).unwrap();
    std::thread::sleep(StdDuration::from_millis(150));
    kill(pid, Signal::SIGSTOP).unwrap();
}

fn write_agent_pid(work: &Path, session: &Session, agent: u32) {
    let (_, key) = NativeBackend::window_title_and_pid_key(session).unwrap();
    let pids = work.join("pids");
    std::fs::create_dir_all(&pids).unwrap();
    let start = process_start_time(agent).unwrap();
    std::fs::write(
        pids.join(format!("{key}.pid")),
        format!("{agent}\n{start}\n"),
    )
    .unwrap();
}

/// An idle older command must not hide a busy one started after it.
#[test]
fn an_idle_older_command_does_not_hide_a_busy_parallel_one() {
    let temp = tempfile::tempdir().unwrap();
    let mut session = Session::new();
    session.id = "parallel-session".to_string();
    session.stage_id = Some("stage-1".to_string());
    let (mut agent, worker) = spawn_agent(
        "sh -c 'sleep 600 & wait' sh &",
        &temp.path().join("worker.pid"),
    );
    write_agent_pid(temp.path(), &session, agent.child().id());
    let mut detection = Detection::new();
    let mut holds = |seconds: i64| {
        let now = session.created_at + TimeDelta::seconds(seconds);
        detection.foreground_command_holds(temp.path(), &session, None, 300, now)
    };

    assert!(holds(301), "a command's first sighting has its grace");
    assert!(!holds(601), "no command did any work");
    burn(worker);
    assert!(holds(602), "the younger command's work earns credit");
    assert!(!holds(902), "and its idle grace runs out");
}

/// A current forward with no fresh evidence holds nothing itself. The CPU its
/// launch burns earns nothing either, and a healthy command beside it does.
#[test]
#[serial]
fn a_forward_with_no_evidence_earns_no_credit_and_blocks_none_beside_it() {
    let fixture = Fixture::new().unwrap();
    let home = fixture.state_root.ancestors().nth(3).unwrap();
    let _env = InstallRootsEnv::isolate(home);
    let start = Utc::now() - TimeDelta::seconds(10);
    let mut row = fixture.authorization_value("unit-1", INV_A);
    row["ts"] = json!((start + TimeDelta::seconds(5))
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string());
    fixture.write_authorizations(&[row]).unwrap();
    let mut session = fixture.session.clone();
    session.created_at = start;

    let forward =
        format!("sh -c 'sh -c \"while :; do :; done\" & wait' sh '--invocation-id {INV_A}' &");
    let (mut agent, worker) = spawn_agent(&forward, &fixture.workspace.join("worker.pid"));
    write_agent_pid(&fixture.work_dir, &session, agent.child().id());
    let at = |seconds: i64| start + TimeDelta::seconds(seconds);
    let evidence = crate::codex_lifecycle::foreground_forward_active(
        &fixture.work_dir,
        &fixture.stage,
        &session.id,
        start,
        Duration::from_secs(300),
        at(301),
    );
    assert_eq!(evidence, Some(false));
    let mut detection = Detection::new();
    let mut holds = |seconds: i64| {
        detection.foreground_command_holds(&fixture.work_dir, &session, None, 300, at(seconds))
    };

    assert!(
        holds(301),
        "the command beside the forward has its first grace"
    );
    std::thread::sleep(StdDuration::from_millis(150));
    assert!(!holds(601), "the forward's own CPU earned credit");
    burn(worker);
    assert!(holds(602), "the forward blocked the command beside it");
}
