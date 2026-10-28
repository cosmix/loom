//! The fake session host and the harness the bridge's unit tests drive: a runtime directory, a
//! bridge over the fake, spool and transcript writers and bounded waits. No tmux.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::{json, Value};

use super::host::{LaunchSpec, SessionHost, Spawned};
pub(super) use super::tests::{
    append_record, assistant_tools, interrupted, pressure, settle, tool_result, wait_for,
    wait_until,
};
use super::TurnPins;
use super::{Bridge, BridgeConfig, BridgeEvent, EndOutcome, LaunchGuidance, StartRequest};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{ChatDelta, DenyReason, Origin, SessionState};
use crate::plan_session::permission::{Answer, Behavior};
use crate::plan_session::spool::append_event;
use crate::planning::runtime::{create_runtime_dir, runtime_paths_under, RuntimePaths};
use crate::process::{IdentityStatus, ProcessIdentity};

pub(super) const OPENING: &str = "OPEN";
pub(super) const CLOSING: &str = "CLOSE";

/// One call the bridge made on the host, by socket.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Call {
    Spawn(String),
    Deliver(String, String),
    Interrupt(String),
    Teardown(String),
}

/// Records every call; identities are VerifiedAlive unless a test sets them.
#[derive(Default)]
pub(super) struct FakeHost {
    calls: Mutex<Vec<Call>>,
    specs: Mutex<Vec<(LaunchSpec, u32)>>,
    identities: Mutex<HashMap<u32, IdentityStatus>>,
    fail_teardown: AtomicBool,
    fail_spawn: AtomicBool,
    fail_record: AtomicBool,
    fail_deliver: AtomicBool,
    next_pid: AtomicU32,
}

impl FakeHost {
    pub(super) fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn log(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }

    pub(super) fn spec(&self, id: &str) -> Option<(LaunchSpec, u32)> {
        let specs = self.specs.lock().unwrap();
        specs.iter().find(|(spec, _)| spec.id == id).cloned()
    }

    pub(super) fn set_identity(&self, pid: u32, status: IdentityStatus) {
        self.identities.lock().unwrap().insert(pid, status);
    }

    pub(super) fn fail_teardown(&self, fail: bool) {
        self.fail_teardown.store(fail, Ordering::SeqCst);
    }

    pub(super) fn fail_spawn(&self) {
        self.fail_spawn.store(true, Ordering::SeqCst);
    }

    /// A spawn swaps `session.json` for a symlink: the no-follow write completing it fails.
    pub(super) fn fail_record(&self) {
        self.fail_record.store(true, Ordering::SeqCst);
    }

    pub(super) fn fail_deliver(&self) {
        self.fail_deliver.store(true, Ordering::SeqCst);
    }

    pub(super) fn delivered(&self, socket: &str) -> Vec<String> {
        let calls = self.calls();
        let texts = calls.into_iter().filter_map(|call| match call {
            Call::Deliver(to, text) if to == socket => Some(text),
            _ => None,
        });
        texts.collect()
    }

    pub(super) fn count(&self, wanted: &Call) -> usize {
        self.calls().iter().filter(|call| *call == wanted).count()
    }
}

impl SessionHost for FakeHost {
    fn spawn(&self, spec: &LaunchSpec) -> anyhow::Result<Spawned> {
        self.log(Call::Spawn(spec.socket.clone()));
        anyhow::ensure!(
            !self.fail_spawn.load(Ordering::SeqCst),
            "scripted spawn failure"
        );
        if self.fail_record.load(Ordering::SeqCst) {
            // Refused for any uid, unlike a read-only mode; the record stays readable.
            let (record, kept) = (spec.dir.record(), spec.dir.path().join("record.kept"));
            fs::rename(&record, &kept)?;
            std::os::unix::fs::symlink(&kept, &record)?;
        }
        let pid = 3_700_000 + self.next_pid.fetch_add(1, Ordering::SeqCst);
        self.specs.lock().unwrap().push((spec.clone(), pid));
        let start_time = Some(42);
        let identity = ProcessIdentity { pid, start_time };
        Ok(Spawned { identity })
    }

    fn deliver(&self, socket: &str, _target: &str, text: &str) -> anyhow::Result<()> {
        self.log(Call::Deliver(socket.to_owned(), text.to_owned()));
        let failing = self.fail_deliver.load(Ordering::SeqCst);
        anyhow::ensure!(!failing, "scripted paste failure");
        Ok(())
    }

    fn interrupt(&self, socket: &str, _target: &str) -> anyhow::Result<()> {
        self.log(Call::Interrupt(socket.to_owned()));
        Ok(())
    }

    fn teardown(&self, socket: &str) -> anyhow::Result<()> {
        self.log(Call::Teardown(socket.to_owned()));
        anyhow::ensure!(
            !self.fail_teardown.load(Ordering::SeqCst),
            "scripted teardown failure"
        );
        Ok(())
    }

    fn identity(&self, identity: ProcessIdentity) -> IdentityStatus {
        let identities = self.identities.lock().unwrap();
        let status = identities.get(&identity.pid).copied();
        status.unwrap_or(IdentityStatus::VerifiedAlive)
    }
}

/// Records each pin key; `end_turn` reports `changes`.
#[derive(Default)]
pub(super) struct FakePins {
    pub(super) begins: Mutex<Vec<String>>,
    pub(super) ends: Mutex<Vec<String>>,
    pub(super) changes: Mutex<Vec<(String, Option<String>)>>,
}

impl TurnPins for FakePins {
    fn begin_turn(&self, turn: &str) {
        self.begins.lock().unwrap().push(turn.to_owned());
    }

    fn end_turn(&self, turn: &str) -> Vec<(String, Option<String>)> {
        self.ends.lock().unwrap().push(turn.to_owned());
        self.changes.lock().unwrap().clone()
    }
}

/// One session as a test sees it.
pub(super) struct Sess {
    pub(super) id: String,
    pub(super) dir: SessionDir,
    pub(super) socket: String,
    pub(super) pid: u32,
    pub(super) transcript: PathBuf,
}

/// A running bridge over the fake host; dropping it stops and joins the driver.
pub(super) struct Harness {
    tmp: tempfile::TempDir,
    pub(super) runtime: RuntimePaths,
    pub(super) host: Arc<FakeHost>,
    pub(super) pins: Arc<FakePins>,
    pub(super) events: Arc<Mutex<Vec<BridgeEvent>>>,
    pub(super) deltas: Arc<Mutex<Vec<ChatDelta>>>,
    pub(super) bridge: Arc<Bridge>,
    stop: Arc<AtomicBool>,
    driver: Option<JoinHandle<()>>,
}

impl Harness {
    /// A bridge whose recovery has not run (`start_session` is Busy until `recover`).
    pub(super) fn new(tweak: impl FnOnce(&mut BridgeConfig)) -> Harness {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        fs::create_dir_all(root.join("doc/plans")).unwrap();
        fs::create_dir_all(tmp.path().join("transcripts")).unwrap();
        let runtime = runtime_paths_under(&tmp.path().join("loom-home"), &root).unwrap();
        create_runtime_dir(&runtime).unwrap();
        let mut config = BridgeConfig {
            project_root: root.clone(),
            runtime: runtime.clone(),
            loom_bin: PathBuf::from("/nonexistent/loom"),
            claude_bin: PathBuf::from("/nonexistent/claude"),
            home: root.join("home"),
            closing_message: Arc::new(|| CLOSING.to_owned()),
            delivery_timeout: Duration::from_secs(10),
            final_write_bound: Duration::from_secs(30),
            permission_deny_after: Duration::from_secs(3590),
            identity_interval: Duration::from_millis(100),
            tick: Duration::from_millis(20),
        };
        tweak(&mut config);
        let (host, pins) = (Arc::new(FakeHost::default()), Arc::new(FakePins::default()));
        let turn_pins: Arc<dyn TurnPins> = pins.clone();
        let bridge = Bridge::new(config, host.clone(), Some(turn_pins));
        let events: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
        let deltas: Arc<Mutex<Vec<ChatDelta>>> = Arc::default();
        let (seen, published) = (events.clone(), deltas.clone());
        bridge.subscribe(Box::new(move |event: &BridgeEvent| {
            seen.lock().unwrap().push(event.clone());
        }));
        bridge.subscribe_chat(Box::new(move |delta: &ChatDelta| {
            published.lock().unwrap().push(delta.clone());
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let driver = Some(bridge.run(stop.clone()));
        Harness {
            tmp,
            runtime,
            host,
            pins,
            events,
            deltas,
            bridge,
            stop,
            driver,
        }
    }

    /// `new` followed by a recovery over the empty sessions directory.
    pub(super) fn ready(tweak: impl FnOnce(&mut BridgeConfig)) -> Harness {
        let harness = Harness::new(tweak);
        harness.bridge.recover();
        harness
    }

    pub(super) fn transcript_of(&self, id: &str) -> PathBuf {
        let transcripts = self.tmp.path().join("transcripts");
        transcripts.join(format!("{id}.jsonl"))
    }

    pub(super) fn request(&self, kind: SessionKind, opening: &str, on_argv: bool) -> StartRequest {
        let opening = opening.to_owned();
        let guidance = Box::new(move |dir: &SessionDir| {
            Ok::<_, anyhow::Error>(LaunchGuidance {
                brief_path: dir.brief(),
                plugin_dir: dir.plugin(),
                opening_message: opening,
                model: "opus".to_owned(),
                effort: "medium".to_owned(),
                display_name: "loom plan: tests".to_owned(),
                opening_on_argv: on_argv,
            })
        });
        let opening_origin = Some(Origin::Start);
        let extra_env = Vec::new();
        StartRequest {
            kind,
            guidance,
            opening_origin,
            offer: None,
            extra_env,
        }
    }

    /// Start a session and wait for its spawn; it is still Starting.
    pub(super) fn start(&self, kind: SessionKind) -> Sess {
        let id = self
            .bridge
            .start_session(self.request(kind, OPENING, false))
            .unwrap();
        self.sess(&id)
    }

    pub(super) fn sess(&self, id: &str) -> Sess {
        let (spec, pid) = wait_for("the spawn", || self.host.spec(id));
        let transcript = self.transcript_of(id);
        fs::write(&transcript, b"").unwrap();
        let (id, dir, socket) = (id.to_owned(), spec.dir, spec.socket);
        Sess {
            id,
            dir,
            socket,
            pid,
            transcript,
        }
    }

    /// Start a session and run its opening turn to its end.
    pub(super) fn idle(&self, kind: SessionKind) -> Sess {
        let s = self.start(kind);
        self.spool(&s, "SessionStart", json!({"source": "startup"}));
        wait_until("the opening", || {
            self.host.delivered(&s.socket) == [OPENING]
        });
        self.working(&s);
        self.spool(&s, "Stop", json!({}));
        self.wait_state(&s, SessionState::Idle);
        s
    }

    pub(super) fn working(&self, s: &Sess) {
        self.spool(s, "UserPromptSubmit", json!({"prompt": "go on"}));
        self.wait_state(s, SessionState::Working);
    }

    /// Append one hook event line the way the hook does.
    pub(super) fn spool(&self, s: &Sess, event: &str, extra: Value) {
        let mut payload = json!({"session_id": s.id, "transcript_path": s.transcript,
            "hook_event_name": event});
        for (key, value) in extra.as_object().unwrap() {
            payload[key] = value.clone();
        }
        append_event(s.dir.path(), event, &payload).unwrap();
        thread::sleep(Duration::from_millis(3));
    }

    pub(super) fn permission(&self, s: &Sess, tool: &str, input: Value) -> String {
        let request = uuid::Uuid::new_v4().to_string();
        let extra = json!({"request_id": request, "tool_name": tool, "tool_input": input});
        self.spool(s, "PermissionRequest", extra);
        request
    }

    pub(super) fn record(&self, s: &Sess, record: Value) {
        append_record(&s.transcript, record, &s.id);
    }

    pub(super) fn answer(&self, s: &Sess, request: &str) -> Option<Answer> {
        let bytes = fs::read(s.dir.answers().join(format!("{request}.json"))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub(super) fn wait_deny(&self, s: &Sess, request: &str, reason: DenyReason) {
        let answer = wait_for("the deny answer", || self.answer(s, request));
        assert_eq!(answer.behavior, Behavior::Deny, "{answer:?}");
        assert_eq!(answer.reason, Some(reason), "{answer:?}");
    }

    /// Texts pasted into the session, the opening excluded.
    pub(super) fn sent(&self, s: &Sess) -> Vec<String> {
        let delivered = self.host.delivered(&s.socket).into_iter();
        delivered.filter(|text| text != OPENING).collect()
    }

    pub(super) fn state(&self, s: &Sess) -> Option<SessionState> {
        let listed = self
            .bridge
            .sessions()
            .into_iter()
            .find(|(id, ..)| *id == s.id);
        listed.map(|(_, _, state)| state).or_else(|| {
            let events = self.events.lock().unwrap();
            events.iter().rev().find_map(|event| match event {
                BridgeEvent::State { id, state } if *id == s.id => Some(*state),
                _ => None,
            })
        })
    }

    pub(super) fn wait_state(&self, s: &Sess, state: SessionState) {
        wait_until(&format!("state {state:?}"), || self.state(s) == Some(state));
    }

    pub(super) fn ended(&self, id: &str) -> Option<EndOutcome> {
        let events = self.events.lock().unwrap();
        events.iter().find_map(|event| match event {
            BridgeEvent::Ended {
                id: ended, outcome, ..
            } if ended == id => Some(outcome.clone()),
            _ => None,
        })
    }

    pub(super) fn count_events(&self, wanted: impl Fn(&BridgeEvent) -> bool) -> usize {
        let events = self.events.lock().unwrap();
        events.iter().filter(|event| wanted(event)).count()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(driver) = self.driver.take() {
            let _ = driver.join();
        }
    }
}
