//! The owner watch leaves the process group its launch script starts it in, so a signal to the
//! pane's group does not end it. No tmux server is involved: the watch only looks at the socket
//! path, so a plain file stands in for the socket. The hook runs with no inherited `LOOM_*`
//! variable.

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use loom::plan_session::bridge::registry::{write_record, SessionRecord};
use loom::plan_session::guard::GuardPolicy;
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::planning::runtime::runtime_paths_under;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::{getsid, Pid};

const ID: &str = "0f8e7d6c-5b4a-4938-8271-605f4e3d2c1b";
/// The socket name the bridge derives from `ID`; `watch_target` refuses any other.
const SOCKET: &str = "loom-plan-0f8e7d6c5b4a";
const DEADLINE: Duration = Duration::from_secs(10);

/// A child killed and reaped however the test ends.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// A session directory the watch accepts, under `base`, with its socket stand-in at the path
/// `TMUX_TMPDIR=base` gives it.
fn consistent_session(base: &Path) -> PathBuf {
    let project = base.join("proj");
    fs::create_dir(&project).unwrap();
    let runtime = runtime_paths_under(&base.join("loom"), &project).unwrap();
    let dir = runtime.sessions.join(ID);
    fs::create_dir_all(&dir).unwrap();
    let policy = GuardPolicy {
        schema: 1,
        kind: SessionKind::Authoring,
        project_root: project,
        runtime_dir: runtime.dir,
        session_dir: dir.clone(),
        home: base.join("home"),
    };
    let record = SessionRecord {
        schema: 1,
        id: ID.to_owned(),
        kind: SessionKind::Authoring,
        socket: SOCKET.to_owned(),
        tmux_session: "loom".to_owned(),
        pid: None,
        start_time: None,
        transcript_path: None,
        started_at: "2026-01-01T00:00:00Z".to_owned(),
        feature: None,
        plan: None,
        children: Vec::new(),
    };
    write_record(&SessionDir::new(dir.clone()), &record).unwrap();
    fs::write(dir.join("guard.json"), serde_json::to_vec(&policy).unwrap()).unwrap();
    let uid = fs::metadata(base).unwrap().uid();
    let sockets = base.join(format!("tmux-{uid}"));
    fs::create_dir(&sockets).unwrap();
    fs::write(sockets.join(SOCKET), b"").unwrap();
    dir
}

fn silent(mut command: Command) -> Child {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + DEADLINE;
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn owner_watch_survives_a_sighup_to_the_group_it_started_in() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let dir = consistent_session(&base);

    // As in the launch script, the watch joins a group another process leads. A process leading
    // its own group could not call setsid.
    let mut sleep = Command::new("sleep");
    sleep.arg("300").process_group(0);
    let mut leader = Reaped(silent(sleep));
    let group = i32::try_from(leader.0.id()).unwrap();
    let mut hook = Command::new(env!("CARGO_BIN_EXE_loom"));
    hook.args(["plan", "hook", "OwnerWatch", "--session-dir"])
        .arg(&dir)
        .env("TMUX_TMPDIR", &base)
        .process_group(group);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            hook.env_remove(key);
        }
    }
    let mut watch = Reaped(silent(hook));
    let pid = Pid::from_raw(i32::try_from(watch.0.id()).unwrap());

    wait_for("the watch never led a session of its own", || {
        assert_eq!(watch.0.try_wait().unwrap(), None, "the watch returned");
        getsid(Some(pid)) == Ok(pid)
    });

    killpg(Pid::from_raw(group), Signal::SIGHUP).unwrap();

    let mut ended = None;
    wait_for("the group's leader outlived the SIGHUP", || {
        ended = leader.0.try_wait().unwrap();
        ended.is_some()
    });
    assert_eq!(
        ended.and_then(|status| status.signal()),
        Some(Signal::SIGHUP as i32)
    );
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        watch.0.try_wait().unwrap(),
        None,
        "the SIGHUP ended the watch"
    );
}
