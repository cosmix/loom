//! The session bridge over real tmux: recovery after a server died (runs wherever tmux can
//! bind its socket), and the operator's end-to-end run with a logged-in Claude Code
//! (`--include-ignored`), which prints what the installed Claude Code actually does.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use loom::orchestrator::terminal::tmux::socket_path_for;
use loom::plan_session::bridge::registry::{write_record, SessionRecord, SCHEMA};
use loom::plan_session::bridge::*;
use loom::plan_session::kind::*;
use loom::plan_session::model::{ChatDelta, Item, Origin, SessionState};
use loom::plan_session::spool::append_event;
use loom::plan_session::tmux_host::TmuxHost;
use loom::planning::runtime::{create_runtime_dir, runtime_paths_under, RuntimePaths};
use loom::process::sandbox_probe::{skip_unless, unix_socket_bindable};
use serde_json::json;
use serial_test::serial;
use tempfile::TempDir;

#[path = "plan_session_bridge_e2e/diagnose.rs"]
mod diagnose;
#[path = "plan_session_bridge_e2e/operator.rs"]
mod operator;

const CLOSING: &str = "Write down what is still open, then stop.";

/// Sets an environment variable for the test and restores it when dropped.
struct EnvGuard(&'static str, Option<OsString>);

impl EnvGuard {
    fn set(name: &'static str, value: &Path) -> EnvGuard {
        let original = std::env::var_os(name);
        std::env::set_var(name, value);
        EnvGuard(name, original)
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.1 {
            Some(value) => std::env::set_var(self.0, value),
            None => std::env::remove_var(self.0),
        }
    }
}

/// Ends the tmux server on `socket` when dropped, also when the test fails.
struct ServerGuard(String);

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.0, "kill-server"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// A bridge, its event and delta logs, and its driver.
struct Running {
    bridge: Arc<Bridge>,
    events: Arc<Mutex<Vec<BridgeEvent>>>,
    deltas: Arc<Mutex<Vec<ChatDelta>>>,
    stop: Arc<AtomicBool>,
    driver: Option<JoinHandle<()>>,
    cleanup_servers: bool,
}

impl Running {
    fn new(config: BridgeConfig) -> Running {
        let bridge = Bridge::new(config, Arc::new(TmuxHost), None);
        let events: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
        let deltas: Arc<Mutex<Vec<ChatDelta>>> = Arc::default();
        let (seen, published) = (events.clone(), deltas.clone());
        bridge.subscribe(Box::new(move |event| {
            println!("event: {event:?}");
            seen.lock().unwrap().push(event.clone());
        }));
        bridge.subscribe_chat(Box::new(move |delta| {
            published.lock().unwrap().push(delta.clone())
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let driver = Some(bridge.run(stop.clone()));
        Running {
            bridge,
            events,
            deltas,
            stop,
            driver,
            cleanup_servers: true,
        }
    }

    fn ended(&self, id: &str) -> Option<EndOutcome> {
        let events = self.events.lock().unwrap();
        events.iter().find_map(|event| match event {
            BridgeEvent::Ended {
                id: ended, outcome, ..
            } if ended == id => Some(outcome.clone()),
            _ => None,
        })
    }

    fn state(&self, id: &str) -> Option<SessionState> {
        let mut sessions = self.bridge.sessions().into_iter();
        sessions
            .find(|(listed, ..)| listed == id)
            .map(|(.., state)| state)
    }

    /// Every item of the chat shown now and of the previous chat.
    fn items(&self) -> Vec<Item> {
        let snapshot = self.bridge.chat_snapshot(true);
        let previous = snapshot
            .previous
            .map(|ended| ended.turns)
            .unwrap_or_default();
        let turns = previous.into_iter().chain(snapshot.turns);
        turns.flat_map(|turn| turn.items).collect()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(driver) = self.driver.take() {
            let _ = driver.join();
        }
        if self.cleanup_servers {
            let sockets: BTreeSet<_> = self
                .bridge
                .sessions()
                .into_iter()
                .map(|(id, ..)| socket_of(&id))
                .collect();
            for socket in sockets {
                drop(ServerGuard(socket));
            }
        }
    }
}

fn wait(what: &str, limit: Duration, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + limit;
    while !ready() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(100));
    }
}

fn socket_of(id: &str) -> String {
    let hex: String = id.chars().filter(|c| *c != '-').take(12).collect();
    format!("loom-plan-{hex}")
}

/// `home` is the HOME the launch gives the session's claude; the launch records the project
/// root as trusted in its `.claude.json`.
fn config(
    root: &Path,
    runtime: &RuntimePaths,
    claude: PathBuf,
    home: &Path,
    bound: Duration,
) -> BridgeConfig {
    BridgeConfig {
        project_root: root.to_path_buf(),
        runtime: runtime.clone(),
        loom_bin: PathBuf::from(env!("CARGO_BIN_EXE_loom")),
        claude_bin: claude,
        home: home.to_path_buf(),
        closing_message: Arc::new(|| CLOSING.to_owned()),
        delivery_timeout: Duration::from_secs(10),
        final_write_bound: bound,
        permission_deny_after: Duration::from_secs(3590),
        identity_interval: Duration::from_millis(200),
        tick: Duration::from_millis(50),
    }
}

/// An authoring start whose guidance writes the brief and a plugin (with `/loom-plan:probe`).
fn request(opening: &str, origin: Option<Origin>, on_argv: bool) -> StartRequest {
    let opening = opening.to_owned();
    let guidance = Box::new(move |dir: &SessionDir| -> anyhow::Result<LaunchGuidance> {
        fs::write(dir.brief(), "You are a test session. Answer briefly.")?;
        let manifest = dir.plugin().join(".claude-plugin");
        fs::create_dir_all(&manifest)?;
        let plugin = r#"{"name": "loom-plan", "version": "0.0.0", "description": "test"}"#;
        fs::write(manifest.join("plugin.json"), plugin)?;
        fs::create_dir_all(dir.plugin().join("commands"))?;
        fs::write(
            dir.plugin().join("commands/probe.md"),
            "Reply with the single word PROBE-OK.",
        )?;
        Ok(LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message: opening,
            model: "sonnet".to_owned(),
            effort: "low".to_owned(),
            display_name: "loom plan: e2e".to_owned(),
            opening_on_argv: on_argv,
        })
    });
    StartRequest {
        kind: SessionKind::Authoring,
        guidance,
        opening_origin: origin,
        offer: None,
        feature: None,
        plan: None,
        extra_env: Vec::new(),
    }
}

/// A project under `base` with `doc/product/index.md`, its runtime directory and a home.
fn project(base: &Path) -> (PathBuf, RuntimePaths) {
    let root = base.join("project");
    fs::create_dir_all(root.join("doc/product")).unwrap();
    fs::create_dir_all(root.join("doc/plans")).unwrap();
    fs::write(
        root.join("doc/product/index.md"),
        "# Demo\n\nA demo product.\n",
    )
    .unwrap();
    let runtime = runtime_paths_under(&base.join("loom-home"), &root).unwrap();
    create_runtime_dir(&runtime).unwrap();
    (root, runtime)
}

/// Server A launches a session over a fake claude that only sleeps, then dies without a
/// teardown; the startup event its hook would have spooled makes the session idle.
fn abandoned_session(
    root: &Path,
    runtime: &RuntimePaths,
    claude: &Path,
    home: &Path,
) -> (String, SessionDir, ServerGuard) {
    let bound = Duration::from_secs(600);
    let mut a = Running::new(config(root, runtime, claude.to_path_buf(), home, bound));
    a.bridge.recover();
    let opening = request("Begin.", Some(Origin::Start), false);
    let id = a.bridge.start_session(opening).unwrap();
    wait("the launch", Duration::from_secs(20), || {
        let events = a.events.lock().unwrap();
        let started =
            |e: &&BridgeEvent| matches!(e, BridgeEvent::Started { id: s, .. } if *s == id);
        events.iter().any(|e| started(&e))
    });
    // Recovery needs the live server after this bridge stops; transfer its cleanup guard.
    let server = ServerGuard(socket_of(&id));
    a.cleanup_servers = false;
    drop(a);
    let dir = SessionDir::new(runtime.sessions.join(&id));
    let start = json!({"hook_event_name": "SessionStart", "session_id": id, "source": "startup"});
    append_event(dir.path(), "SessionStart", &start).unwrap();
    (id, dir, server)
}

/// A record written by hand for a live pid with no start time: its identity is unverifiable.
fn unverifiable_session(runtime: &RuntimePaths) -> (String, SessionDir) {
    let id = uuid::Uuid::new_v4().to_string();
    let dir = SessionDir::new(runtime.sessions.join(&id));
    fs::create_dir_all(dir.path()).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let record = SessionRecord {
        schema: SCHEMA,
        id: id.clone(),
        kind: SessionKind::Authoring,
        socket: socket_of(&id),
        tmux_session: "plan-authoring".to_owned(),
        pid: Some(std::process::id()),
        start_time: None,
        transcript_path: None,
        started_at: "2026-10-28T00:00:00.000Z".to_owned(),
        children: Vec::new(),
        feature: None,
        plan: None,
    };
    write_record(&dir, &record).unwrap();
    (id, dir)
}

fn tmux_available(sockets: &Path, test: &str) -> bool {
    let tmux = Command::new("tmux").arg("-V").stdin(Stdio::null()).output();
    let tmux = tmux.is_ok_and(|output| output.status.success());
    let why = "needs a tmux binary and a sandbox that lets it bind its unix socket";
    !skip_unless(tmux && unix_socket_bindable(sockets), test, why)
}

#[test]
#[serial]
fn recovery_closes_a_live_session_and_leaves_an_unverifiable_one() {
    let test = "recovery_closes_a_live_session_and_leaves_an_unverifiable_one";
    let sockets = tempfile::Builder::new()
        .tempdir_in("/tmp")
        .or_else(|_| TempDir::new());
    let sockets = sockets.unwrap();
    if !tmux_available(sockets.path(), test) {
        return;
    }
    let _tmux_dir = EnvGuard::set("TMUX_TMPDIR", sockets.path());
    let base = TempDir::new().unwrap();
    let base = base.path().canonicalize().unwrap();
    let _loom_home = EnvGuard::set("LOOM_HOME", &base.join("loom-home"));
    let (root, runtime) = project(&base);
    // A temp HOME: the launch writes its trust entry there, never into the developer's own.
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let claude = base.join("claude");
    fs::write(&claude, "#!/bin/sh\nexec sleep 600\n").unwrap();
    fs::set_permissions(&claude, fs::Permissions::from_mode(0o755)).unwrap();
    let (id, dir, _server) = abandoned_session(&root, &runtime, &claude, &home);
    let (other, other_dir) = unverifiable_session(&runtime);

    let bound = Duration::from_secs(2);
    let b = Running::new(config(&root, &runtime, claude, &home, bound));
    let report = b.bridge.recover();
    assert!(report.closed.iter().any(|s| s.id == id), "{report:?}");
    assert!(
        !socket_path_for(&socket_of(&id)).exists(),
        "the socket survived"
    );
    assert!(!dir.path().exists(), "the directory survived");
    let deltas = serde_json::to_string(&*b.deltas.lock().unwrap()).unwrap();
    assert!(
        deltas.contains(CLOSING),
        "the closing message was not pasted"
    );
    let left = report.left_alone.iter().find(|(left, ..)| *left == other);
    let (_, socket, why) = left.unwrap_or_else(|| panic!("{other} not left alone: {report:?}"));
    assert_eq!(*socket, socket_of(&other), "{why}");
    assert!(
        other_dir.record().exists(),
        "the unverifiable session was touched"
    );
}
