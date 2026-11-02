//! Both public consumers read the same live process and persisted CPU history.

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::*;
use crate::orchestrator::monitor::detection::Detection;
use crate::orchestrator::monitor::heartbeat::write_heartbeat;
use crate::orchestrator::terminal::native::NativeBackend;
use crate::process::{process_start_time, OwnedProcess};

struct LiveFixture {
    _tmp: tempfile::TempDir,
    _agent: OwnedProcess,
    work: WorkDir,
    session: Session,
    stage: Stage,
    heartbeat: Heartbeat,
    worker: u32,
    projects: std::path::PathBuf,
}

impl LiveFixture {
    fn new(after_progress: bool) -> Self {
        let tmp = tempfile::TempDir::new().unwrap();
        let work = WorkDir::new(tmp.path().join("work")).unwrap();
        work.initialize().unwrap();
        let worker_path = tmp.path().join("worker.pid");
        let (mut agent, worker) = spawn_worker(&worker_path);
        let mut session = Session::new();
        session.stage_id = Some("stage-1".into());
        session.status = SessionStatus::Running;
        let stage = Stage {
            id: "stage-1".into(),
            status: StageStatus::Executing,
            subagent_timeout_secs: Some(300),
            ..Stage::default()
        };
        let mut heartbeat = Heartbeat::new(stage.id.clone(), session.id.clone());
        let projects = write_transcript(tmp.path(), &session.id, &mut heartbeat);
        heartbeat.timestamp = session.created_at;
        heartbeat.progress_at = Some(session.created_at);
        if after_progress {
            heartbeat.last_tool = Some("Read".into());
        }
        write_heartbeat(work.root(), &heartbeat).unwrap();
        let (_, key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        std::fs::write(
            work.root().join("pids").join(format!("{key}.pid")),
            format!(
                "{}\n{}\n",
                agent.child().id(),
                process_start_time(agent.child().id()).unwrap()
            ),
        )
        .unwrap();
        Self {
            _tmp: tmp,
            _agent: agent,
            work,
            session,
            stage,
            heartbeat,
            worker,
            projects,
        }
    }

    fn assert_consumers(&self, monitor: &mut Detection, seconds: i64, busy: bool) {
        let now = self.session.created_at + chrono::TimeDelta::seconds(seconds);
        let held = monitor.foreground_command_holds(
            self.work.root(),
            &self.session,
            Some(&self.heartbeat),
            300,
            now,
        );
        let facts = heartbeat_facts_at(
            &self.stage,
            Some(&self.session),
            &self.work,
            Some(&self.projects),
            now,
        );
        assert_eq!(held, busy, "monitor at {seconds}");
        assert_eq!(
            facts.activity_status,
            if busy {
                ActivityStatus::Busy
            } else {
                ActivityStatus::Stale
            },
            "status at {seconds}"
        );
        if busy {
            assert_eq!(facts.last_tool.as_deref(), Some("Bash"));
        }
    }
}

fn write_transcript(root: &Path, session: &str, heartbeat: &mut Heartbeat) -> std::path::PathBuf {
    let projects = root.join("projects");
    std::fs::create_dir_all(projects.join("project")).unwrap();
    let transcript = projects.join("project").join(format!("{session}.jsonl"));
    std::fs::write(&transcript, r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#).unwrap();
    heartbeat.transcript_path = Some(transcript.to_string_lossy().into_owned());
    projects
}

fn spawn_worker(path: &Path) -> (OwnedProcess, u32) {
    let mut child = Command::new("sh")
        .args(["-c", "read line; sh -c 'sh -c \"while :; do :; done\" & echo $! > \"$1\"; wait' sh \"$1\" & wait", "sh"])
        .arg(path).stdin(Stdio::piped()).process_group(0).spawn().unwrap();
    std::thread::sleep(Duration::from_millis(25));
    child.stdin.take().unwrap().write_all(b"go\n").unwrap();
    let agent = OwnedProcess::new(child);
    let worker = wait_worker(path);
    signal(worker, nix::sys::signal::Signal::SIGSTOP);
    (agent, worker)
}

fn signal(pid: u32, signal: nix::sys::signal::Signal) {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), signal).unwrap();
    if signal == nix::sys::signal::Signal::SIGSTOP {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .unwrap()
            .contains(") T ")
        {
            assert!(std::time::Instant::now() < deadline, "worker did not stop");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn wait_worker(path: &Path) -> u32 {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(pid) = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            return pid;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "worker failed to start"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn monitor_and_status_share_first_and_later_cpu_progress_at_identical_clocks() {
    for after_progress in [false, true] {
        let fixture = LiveFixture::new(after_progress);
        let mut monitor = Detection::new();
        fixture.assert_consumers(&mut monitor, 301, true);
        fixture.assert_consumers(&mut monitor, 601, false);
        signal(fixture.worker, nix::sys::signal::Signal::SIGCONT);
        std::thread::sleep(Duration::from_millis(150));
        signal(fixture.worker, nix::sys::signal::Signal::SIGSTOP);
        fixture.assert_consumers(&mut monitor, 602, true);
        fixture.assert_consumers(&mut monitor, 899, true);
        fixture.assert_consumers(&mut monitor, 900, true);
        signal(fixture.worker, nix::sys::signal::Signal::SIGCONT);
        std::thread::sleep(Duration::from_millis(150));
        signal(fixture.worker, nix::sys::signal::Signal::SIGSTOP);
        fixture.assert_consumers(&mut monitor, 1000, true);
        fixture.assert_consumers(&mut monitor, 1300, false);
        signal(fixture.worker, nix::sys::signal::Signal::SIGCONT);
        std::thread::sleep(Duration::from_millis(150));
        signal(fixture.worker, nix::sys::signal::Signal::SIGSTOP);
        fixture.assert_consumers(&mut monitor, 1560, false);
    }
}
