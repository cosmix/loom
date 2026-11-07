//! The bridge harness: a runtime directory, a bridge driven by the scripted host, spool and
//! transcript writers, and bounded waits.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::ops::Deref;
use std::os::unix::fs::DirBuilderExt;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use loom::plan_session::bridge::registry::{write_record, SessionRecord};
use loom::plan_session::bridge::{
    Bridge, BridgeConfig, BridgeError, BridgeEvent, EndCause, EndMode, EndOutcome, LaunchGuidance,
    SendOutcome, StartRequest, TurnPins,
};
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::plan_session::model::{DenyReason, SessionState};
use loom::plan_session::permission::{Answer, Behavior};
use loom::plan_session::spool::append_event;
use loom::planning::runtime::{create_runtime_dir, runtime_paths_under, RuntimePaths};
use serde_json::{json, Value};

pub use super::host::*;
pub use super::util::*;

pub const OPENING: &str = "OPEN";
pub const CLOSING: &str = "CLOSE";

/// One live (or recorded) session as a test sees it.
pub struct Sess {
    pub id: String,
    pub dir: SessionDir,
    pub socket: String,
    pub pid: u32,
    pub transcript: PathBuf,
}

/// The bridge events a contract asserts on.
#[derive(Debug, Clone, PartialEq)]
pub enum Ev {
    State(String, SessionState),
    TurnEnded(String),
    Ended(String, &'static str),
}

/// Everything that exists before `Bridge::new`.
pub struct Fixture {
    _tmp: tempfile::TempDir,
    pub root: PathBuf,
    pub runtime: RuntimePaths,
    transcripts: PathBuf,
    pub host: Arc<ScriptedHost>,
    pub pins: Arc<RecordingPins>,
    pub events: Arc<Mutex<Vec<Ev>>>,
}

/// A running bridge over a fixture; dropping it stops and joins the driver.
pub struct Harness {
    fixture: Fixture,
    pub bridge: Arc<Bridge>,
    stop: Arc<AtomicBool>,
    driver: Option<JoinHandle<()>>,
}

impl Fixture {
    pub fn new() -> Fixture {
        socket_dir();
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        let transcripts = tmp.path().join("transcripts");
        fs::create_dir_all(root.join("doc/plans")).unwrap();
        fs::create_dir_all(&transcripts).unwrap();
        let runtime = runtime_paths_under(&tmp.path().join("loom-home"), &root).unwrap();
        create_runtime_dir(&runtime).unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&runtime.sessions)
            .unwrap();
        Fixture {
            _tmp: tmp,
            root,
            runtime,
            transcripts,
            host: Arc::default(),
            pins: Arc::default(),
            events: Arc::default(),
        }
    }

    fn config(&self) -> BridgeConfig {
        BridgeConfig {
            project_root: self.root.clone(),
            runtime: self.runtime.clone(),
            loom_bin: PathBuf::from("/nonexistent/loom"),
            claude_bin: PathBuf::from("/nonexistent/claude"),
            home: self.root.join("home"),
            closing_message: Arc::new(|| CLOSING.to_owned()),
            delivery_timeout: Duration::from_secs(10),
            final_write_bound: Duration::from_secs(30),
            permission_deny_after: Duration::from_secs(3590),
            identity_interval: Duration::from_millis(100),
            tick: Duration::from_millis(20),
            fetch_policy: Default::default(),
        }
    }

    /// Build the bridge, subscribe the event recorder and start the driver; no recovery.
    pub fn start(self, tweak: impl FnOnce(&mut BridgeConfig)) -> Harness {
        let mut config = self.config();
        tweak(&mut config);
        let pins: Arc<dyn TurnPins> = self.pins.clone();
        let bridge = Bridge::new(config, self.host.clone(), Some(pins));
        let events = self.events.clone();
        bridge.subscribe(Box::new(move |event: &BridgeEvent| {
            if let Some(ev) = summarize(event) {
                events.lock().unwrap().push(ev);
            }
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let driver = Some(bridge.run(stop.clone()));
        Harness {
            fixture: self,
            bridge,
            stop,
            driver,
        }
    }

    /// `start` followed by a recovery over the empty sessions directory.
    pub fn ready(self, tweak: impl FnOnce(&mut BridgeConfig)) -> Harness {
        let harness = self.start(tweak);
        harness.bridge.recover();
        harness
    }

    fn session(&self, id: &str, socket: String, pid: u32) -> Sess {
        let transcript = self.transcripts.join(format!("{id}.jsonl"));
        fs::write(&transcript, b"").unwrap();
        let dir = SessionDir::new(self.runtime.sessions.join(id));
        Sess {
            id: id.to_owned(),
            dir,
            socket,
            pid,
            transcript,
        }
    }

    /// A session directory (with `answers/`) and `session.json`, as a crashed server left them.
    pub fn write_record(&self, kind: SessionKind, pid: Option<u32>) -> Sess {
        let id = uuid::Uuid::new_v4().to_string();
        let socket = format!("loom-plan-{}", &id.replace('-', "")[..12]);
        let s = self.session(&id, socket, pid.unwrap_or(0));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(s.dir.path())
            .unwrap();
        fs::DirBuilder::new()
            .mode(0o700)
            .create(s.dir.answers())
            .unwrap();
        let record = SessionRecord {
            schema: 1,
            id: id.clone(),
            tmux_session: format!("plan-{}", kind.label()),
            kind,
            socket: s.socket.clone(),
            pid,
            start_time: pid.map(|_| 42),
            transcript_path: Some(s.transcript.clone()),
            started_at: Utc::now().to_rfc3339(),
            children: Vec::new(),
            feature: None,
            plan: None,
        };
        write_record(&s.dir, &record).unwrap();
        s
    }

    /// A `sessions/<id>/` directory with no record in it.
    pub fn bare_dir(&self) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let dir = self.runtime.sessions.join(&id);
        fs::DirBuilder::new().mode(0o700).create(dir).unwrap();
        id
    }

    /// Append one hook event line the way `loom plan hook <event>` does.
    pub fn spool(&self, s: &Sess, event: &str, extra: Value) {
        let mut payload = json!({
            "session_id": s.id,
            "transcript_path": s.transcript,
            "hook_event_name": event,
        });
        for (key, value) in extra.as_object().unwrap() {
            payload[key] = value.clone();
        }
        append_event(s.dir.path(), event, &payload).unwrap();
        // Distinct `received_at` millis: the bridge skips an event key it already applied.
        thread::sleep(Duration::from_millis(3));
    }

    /// Append a `PermissionRequest` line and return its request id.
    pub fn permission(&self, s: &Sess, tool: &str, input: Value) -> String {
        let request = uuid::Uuid::new_v4().to_string();
        let extra = json!({"request_id": request, "tool_name": tool, "tool_input": input});
        self.spool(s, "PermissionRequest", extra);
        request
    }

    /// Append one main-transcript record, filling the fields every record carries.
    pub fn record(&self, s: &Sess, mut record: Value) {
        record["uuid"] = json!(uuid::Uuid::new_v4().to_string());
        record["parentUuid"] = Value::Null;
        record["isSidechain"] = json!(false);
        record["sessionId"] = json!(s.id);
        record["timestamp"] = json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
        let mut file = OpenOptions::new().append(true).open(&s.transcript).unwrap();
        writeln!(file, "{record}").unwrap();
    }

    pub fn answer(&self, s: &Sess, request: &str) -> Option<Answer> {
        let bytes = fs::read(s.dir.answers().join(format!("{request}.json"))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn wait_deny(&self, s: &Sess, request: &str, reason: DenyReason) {
        let answer = wait_for("the deny answer file", || self.answer(s, request));
        assert_eq!(answer.behavior, Behavior::Deny, "{answer:?}");
        assert_eq!(answer.reason, Some(reason), "{answer:?}");
    }

    /// Texts delivered to the session's pane, the opening message excluded.
    pub fn sent(&self, s: &Sess) -> Vec<String> {
        let delivered = self.host.delivered_to(&s.socket);
        delivered
            .into_iter()
            .filter(|text| text != OPENING)
            .collect()
    }

    pub fn ended(&self, id: &str) -> Option<&'static str> {
        let events = self.events.lock().unwrap();
        events.iter().find_map(|ev| match ev {
            Ev::Ended(ended, outcome) if ended == id => Some(*outcome),
            _ => None,
        })
    }

    pub fn turns_ended(&self, s: &Sess) -> usize {
        let events = self.events.lock().unwrap();
        events
            .iter()
            .filter(|ev| matches!(ev, Ev::TurnEnded(id) if *id == s.id))
            .count()
    }
}

impl Harness {
    /// Start a session and wait for its spawn; the session is still Starting.
    pub fn start(&self, kind: SessionKind) -> Sess {
        let guidance = Box::new(|dir: &SessionDir| {
            Ok::<_, anyhow::Error>(LaunchGuidance {
                brief_path: dir.brief(),
                plugin_dir: dir.plugin(),
                opening_message: OPENING.to_owned(),
                model: "opus".to_owned(),
                effort: "medium".to_owned(),
                display_name: "loom plan: contracts".to_owned(),
                opening_on_argv: false,
            })
        });
        let request = StartRequest {
            kind,
            guidance,
            opening_origin: None,
            offer: None,
            feature: None,
            plan: None,
            extra_env: Vec::new(),
        };
        let id = self.bridge.start_session(request).unwrap();
        let (socket, pid) = wait_for("the spawn", || self.host.spawned(&id));
        self.session(&id, socket, pid)
    }

    /// Start a session and run its opening turn to the end: Idle, nothing queued or delivering.
    pub fn idle(&self, kind: SessionKind) -> Sess {
        let s = self.start(kind);
        self.spool(&s, "SessionStart", json!({"source": "startup"}));
        wait_until("the opening delivery", || {
            self.host.delivered_to(&s.socket) == [OPENING]
        });
        self.working(&s);
        self.spool(&s, "Stop", json!({"stop_hook_active": false}));
        self.wait_state(&s, SessionState::Idle);
        s
    }

    /// A `UserPromptSubmit` line, then wait for Working.
    pub fn working(&self, s: &Sess) {
        self.spool(s, "UserPromptSubmit", json!({"prompt": "go on"}));
        self.wait_state(s, SessionState::Working);
    }

    /// The state `sessions()` lists, else the last `State` event of a session it no longer lists.
    pub fn state(&self, s: &Sess) -> Option<SessionState> {
        let sessions = self.bridge.sessions();
        let listed = sessions.into_iter().find(|(id, ..)| *id == s.id);
        listed.map(|(_, _, state)| state).or_else(|| {
            let events = self.events.lock().unwrap();
            events.iter().rev().find_map(|ev| match ev {
                Ev::State(id, state) if *id == s.id => Some(*state),
                _ => None,
            })
        })
    }

    pub fn wait_state(&self, s: &Sess, state: SessionState) {
        wait_until(&format!("state {state:?}"), || self.state(s) == Some(state));
    }

    pub fn interrupt(&self, s: &Sess) -> Result<Vec<String>, BridgeError> {
        self.bridge.interrupt(&s.id)
    }

    pub fn end(&self, s: &Sess, mode: EndMode, cause: EndCause) -> Vec<String> {
        self.bridge.end(&s.id, mode, cause).unwrap()
    }

    pub fn send(&self, s: &Sess, text: &str) -> SendOutcome {
        self.bridge
            .send(&s.id, text.to_owned(), Vec::new())
            .unwrap()
    }
}

impl Deref for Harness {
    type Target = Fixture;

    fn deref(&self) -> &Fixture {
        &self.fixture
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(driver) = self.driver.take() {
            let _ = driver.join();
        }
    }
}

fn summarize(event: &BridgeEvent) -> Option<Ev> {
    Some(match event {
        BridgeEvent::State { id, state } => Ev::State(id.clone(), *state),
        BridgeEvent::TurnEnded { id, .. } => Ev::TurnEnded(id.clone()),
        BridgeEvent::Ended { id, outcome, .. } => Ev::Ended(id.clone(), outcome_label(outcome)),
        _ => return None,
    })
}

fn outcome_label(outcome: &EndOutcome) -> &'static str {
    match outcome {
        EndOutcome::Closed => "closed",
        EndOutcome::Marker => "marker",
        EndOutcome::TornDown => "torn_down",
        EndOutcome::Failed(_) => "failed",
    }
}

/// Create the pressure completion marker.
pub fn touch_marker(s: &Sess) {
    fs::create_dir_all(s.dir.done()).unwrap();
    fs::write(s.dir.marker(), b"").unwrap();
}
