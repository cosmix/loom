//! Chat wiring over a fake session host; `tests_dispatch` shares this harness.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use serde_json::{json, Value};

use super::guidance::{FixedGuidance, GuidanceProvider, GuidanceSlot};
use super::install_with;
use super::lease::AuthoringLeases;
use crate::plan_server::authoring::{Take, GRACE};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::{BrowserId, ConnCtx};
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_server::wire::server::{MissingPrereq, PrereqKind};
use crate::plan_session::bridge::host::{LaunchSpec, SessionHost, Spawned};
use crate::plan_session::bridge::{
    Bridge, BridgeEvent, EndCause, EndMode, EndOutcome, StartRequest,
};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::SessionState;
use crate::plan_session::spool::append_event;
use crate::planning::runtime::{create_runtime_dir, runtime_paths_under};
use crate::planning::watch::OwnWrites;
use crate::process::{IdentityStatus, ProcessIdentity};

/// Records launches and deliveries; every process is alive; spawns fail when told to.
#[derive(Default)]
pub(super) struct FakeHost {
    delivered: Mutex<Vec<(String, String)>>,
    specs: Mutex<Vec<LaunchSpec>>,
    pub(super) fail_spawn: AtomicBool,
    next_pid: AtomicU32,
}

impl FakeHost {
    pub(super) fn delivered_to(&self, socket: &str) -> Vec<String> {
        let delivered = self.delivered.lock().unwrap();
        let to = delivered.iter().filter(|(to, _)| to == socket);
        to.map(|(_, text)| text.clone()).collect()
    }
}

impl SessionHost for FakeHost {
    fn spawn(&self, spec: &LaunchSpec) -> anyhow::Result<Spawned> {
        anyhow::ensure!(!self.fail_spawn.load(Ordering::SeqCst), "scripted failure");
        self.specs.lock().unwrap().push(spec.clone());
        let pid = 3_800_000 + self.next_pid.fetch_add(1, Ordering::SeqCst);
        let start_time = Some(42);
        let identity = ProcessIdentity { pid, start_time };
        Ok(Spawned { identity })
    }

    fn deliver(&self, socket: &str, _target: &str, text: &str) -> anyhow::Result<()> {
        let call = (socket.to_owned(), text.to_owned());
        self.delivered.lock().unwrap().push(call);
        Ok(())
    }

    fn interrupt(&self, _socket: &str, _target: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn teardown(&self, _socket: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn identity(&self, _identity: ProcessIdentity) -> IdentityStatus {
        IdentityStatus::VerifiedAlive
    }
}

/// One open tab; a thread keeps its frames, so the hub's queue never fills.
pub(super) struct Tab {
    pub(super) conn: ConnCtx,
    /// The `on_open` frames.
    pub(super) opened: Vec<Value>,
    log: Arc<Mutex<Vec<Value>>>,
}

impl Tab {
    pub(super) fn frames(&self) -> Vec<Value> {
        self.log.lock().unwrap().clone()
    }
}

/// A planning context with the chat installed over the fake host, recovery done.
pub(super) struct Fixture {
    tmp: tempfile::TempDir,
    pub(super) ctx: Arc<PlanContext>,
    pub(super) host: Arc<FakeHost>,
    pub(super) bridge: Arc<Bridge>,
    pub(super) events: Arc<Mutex<Vec<BridgeEvent>>>,
}

impl Fixture {
    pub(super) fn new(read_only: bool) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("project");
        std::fs::create_dir_all(root.join("doc/product")).unwrap();
        std::fs::write(root.join("doc/product/index.md"), "# Demo\n").unwrap();
        std::fs::create_dir_all(tmp.path().join("transcripts")).unwrap();
        let (message, fix) = ("tmux is missing".into(), "install tmux".into());
        let problem = MissingPrereq {
            what: PrereqKind::Tmux,
            message,
            fix,
        };
        let read_only = ReadOnly::from_problems(read_only.then_some(problem).into_iter().collect());
        let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7272);
        let bind = BindInfo::new(local, &"ab".repeat(32));
        let runtime = runtime_paths_under(&tmp.path().join("loom-home"), &root).unwrap();
        create_runtime_dir(&runtime).unwrap();
        let ctx = PlanContext::new(root.clone(), read_only, bind, OwnWrites::default());
        let ctx = ctx.unwrap().with_runtime(runtime.clone());
        let (ctx, host) = (Arc::new(ctx), Arc::new(FakeHost::default()));
        install_with(&ctx, host.clone(), move |config| {
            config.runtime = runtime;
            config.home = root.join("home");
            config.claude_bin = "/nonexistent/claude".into(); // FakeHost never runs it
            config.tick = Duration::from_millis(20);
            config.identity_interval = Duration::from_millis(100);
            config.final_write_bound = Duration::from_secs(5);
        });
        let bridge = ctx.extensions().get::<Bridge>().unwrap();
        let events: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
        let seen = events.clone();
        bridge.subscribe(Box::new(move |event| {
            seen.lock().unwrap().push(event.clone())
        }));
        let (tx, rx) = mpsc::channel();
        bridge.on_recovered(Box::new(move |_| tx.send(()).unwrap_or_default()));
        rx.recv_timeout(Duration::from_secs(5)).expect("recovery");
        Fixture {
            tmp,
            ctx,
            host,
            bridge,
            events,
        }
    }

    /// Open a tab of `browser`, counted by authoring as an open connection.
    pub(super) fn connect(&self, browser: &BrowserId) -> Tab {
        let conn = self.ctx.conn_ctx(browser.clone());
        let rx = self.ctx.hub().register(&conn);
        self.ctx.authoring().connection_opened(browser);
        let parse = |frame: &str| serde_json::from_str::<Value>(frame).unwrap();
        let opened = self.ctx.hub().open_frames(&conn);
        let opened = opened.iter().map(|frame| parse(frame)).collect();
        let log: Arc<Mutex<Vec<Value>>> = Arc::default();
        let keep = log.clone();
        thread::spawn(move || rx.iter().for_each(|f| keep.lock().unwrap().push(parse(&f))));
        Tab { conn, opened, log }
    }
    /// Dispatch `cmd` from `tab` and return its `result` frame.
    pub(super) fn command(&self, tab: &Tab, cmd: ChatCommand) -> Value {
        let before = tab.frames().len();
        super::dispatch(&self.ctx, &tab.conn, cmd);
        wait_for("the result frame", || {
            let mut after = tab.frames().into_iter().skip(before);
            after.find(|frame| frame["frame"] == "result")
        })
    }
    /// Start a New chat from `tab` and wait for its launch.
    pub(super) fn start(&self, tab: &Tab) -> LaunchSpec {
        assert_eq!(self.command(tab, start()), ok_result(), "start refused");
        let id = wait_for("the chat", || self.bridge.chat_session_id());
        self.launched(&id)
    }
    pub(super) fn launched(&self, id: &str) -> LaunchSpec {
        wait_for("the spawn", || {
            let specs = self.host.specs.lock().unwrap();
            specs.iter().find(|spec| spec.id == id).cloned()
        })
    }
    /// Start a New chat and run its opening turn to Idle.
    pub(super) fn idle_chat(&self, tab: &Tab) -> LaunchSpec {
        let s = self.start(tab);
        self.spool(&s, "SessionStart", json!({"source": "startup"}));
        let opened = || self.host.delivered_to(&s.socket).len() == 1;
        wait_until("the opening", opened);
        self.spool(&s, "UserPromptSubmit", json!({"prompt": "opening"}));
        self.wait_state(&s, SessionState::Working);
        self.spool(&s, "Stop", json!({}));
        self.wait_state(&s, SessionState::Idle);
        s
    }
    /// Append one hook event the way the hook does.
    pub(super) fn spool(&self, s: &LaunchSpec, event: &str, extra: Value) {
        let transcript = self.tmp.path().join(format!("transcripts/{}.jsonl", s.id));
        if !transcript.exists() {
            std::fs::write(&transcript, b"").unwrap();
        }
        let mut payload = json!({"session_id": s.id, "transcript_path": transcript,
            "hook_event_name": event});
        for (key, value) in extra.as_object().unwrap() {
            payload[key] = value.clone();
        }
        append_event(s.dir.path(), event, &payload).unwrap();
        thread::sleep(Duration::from_millis(3));
    }
    pub(super) fn state(&self, s: &LaunchSpec) -> Option<SessionState> {
        let mut sessions = self.bridge.sessions().into_iter();
        sessions
            .find(|(id, ..)| *id == s.id)
            .map(|(.., state)| state)
    }
    pub(super) fn wait_state(&self, s: &LaunchSpec, state: SessionState) {
        wait_until(&format!("state {state:?}"), || self.state(s) == Some(state));
    }
    pub(super) fn ended(&self, id: &str) -> Vec<EndOutcome> {
        let events = self.events.lock().unwrap();
        let ended = events.iter().filter_map(|event| match event {
            BridgeEvent::Ended {
                id: ended, outcome, ..
            } if ended == id => Some(outcome.clone()),
            _ => None,
        });
        ended.collect()
    }

    /// Wait for the closing message, then run its turn to the Stop that ends the chat.
    fn finish_final_write(&self, s: &LaunchSpec) {
        let closing = self.ctx.extensions().get::<GuidanceSlot>().unwrap();
        let closing = closing.get().closing_message();
        let pasted = || self.host.delivered_to(&s.socket).contains(&closing);
        wait_until("the closing message", pasted);
        self.spool(s, "UserPromptSubmit", json!({"prompt": closing}));
        self.spool(s, "Stop", json!({}));
    }

    /// The holder's last tab closes and the grace period passes.
    pub(super) fn expire_grace(&self, holder: &BrowserId) {
        let now = SystemTime::now();
        self.ctx.authoring().connection_closed(holder, now);
        let later = now + GRACE + Duration::from_secs(1);
        self.ctx.authoring().tick(later);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.bridge.abort();
        let _ = std::fs::remove_dir_all(&self.ctx.runtime().dir);
    }
}

pub(super) fn browser() -> BrowserId {
    BrowserId::mint().unwrap()
}

pub(super) fn start() -> ChatCommand {
    ChatCommand::Start {
        client_id: Some("c1".into()),
        offer: None,
        feature: None,
        plan: None,
        text: None,
        attachments: Vec::new(),
    }
}

pub(super) fn ok_result() -> Value {
    json!({"type": "chat", "frame": "result", "client_id": "c1", "ok": true, "error": null})
}

pub(super) fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn wait_until(what: &str, mut f: impl FnMut() -> bool) {
    wait_for(what, || f().then_some(()));
}

/// The queue entries of every chat frame that carries a queue.
fn queues(frames: &[Value]) -> Vec<Value> {
    let carries = |f: &&Value| f["op"] == "queue" || f["frame"] == "snapshot";
    let entries = frames.iter().filter(carries);
    entries
        .filter_map(|f| f["queue"].as_array().cloned())
        .flatten()
        .collect()
}

#[test]
fn queued_texts_reach_only_the_holders_tabs() {
    let f = Fixture::new(false);
    let (holder, other) = (browser(), browser());
    let tab = f.connect(&holder);
    let watcher = f.connect(&other);
    f.start(&tab);
    let send = ChatCommand::Send {
        client_id: Some("c1".into()),
        text: "first".into(),
        attachments: Vec::new(),
    };
    assert_eq!(f.command(&tab, send), ok_result(), "not queued");
    wait_until("the queue delta", || !queues(&watcher.frames()).is_empty());
    let (late, second) = (f.connect(&other), f.connect(&holder));
    let hidden = [queues(&watcher.frames()), queues(&late.opened)].concat();
    assert!(hidden.iter().all(|e| e["text"].is_null()), "{hidden:?}");
    assert!(hidden.iter().any(|e| e["first_line"] == "first"));
    assert_eq!(queues(&second.opened)[0]["text"], "first");
    let shown = || queues(&tab.frames()).iter().any(|e| e["text"] == "first");
    wait_until("the holder's queue delta", shown);
}

#[test]
fn grace_expiry_ends_the_chat_with_a_final_write_and_a_new_holder_keeps_authoring() {
    let f = Fixture::new(false);
    let (first, second) = (browser(), browser());
    let s = f.idle_chat(&f.connect(&first));
    f.expire_grace(&first);
    f.wait_state(&s, SessionState::Ending);
    let taken = f.ctx.require_authoring(&second, Take::TakeIfFree);
    assert_eq!(taken, Ok(()), "the second browser cannot take authoring");
    f.finish_final_write(&s);
    wait_until("the end", || f.ended(&s.id) == [EndOutcome::Closed]);
    let held = f.ctx.authoring().is_holder(&second);
    assert!(held, "the chat's end freed the new holder");
}

#[test]
fn the_shutdown_drain_ends_the_chat_with_a_final_write() {
    let f = Fixture::new(false);
    let s = f.idle_chat(&f.connect(&browser()));
    let drains = f.ctx.shutdown().begin(|_: &str| {});
    f.finish_final_write(&s);
    drains.join().unwrap();
    assert_eq!(f.ended(&s.id), [EndOutcome::Closed]);
}

#[test]
fn a_failed_chat_releases_authoring_unless_a_lease_holds_it() {
    let f = Fixture::new(false);
    let holder = browser();
    let tab = f.connect(&holder);
    f.host.fail_spawn.store(true, Ordering::SeqCst);
    // Every end is a failed launch; its release runs before the fixture's listener sees `Ended`.
    let fail = |count: usize| {
        assert_eq!(f.command(&tab, start()), ok_result());
        wait_until("the failure", || {
            let events = f.events.lock().unwrap();
            let ended = |e: &&BridgeEvent| matches!(e, BridgeEvent::Ended { .. });
            events.iter().filter(ended).count() == count
        });
    };
    fail(1);
    assert_eq!(f.ctx.authoring().holder(), None, "kept authoring");
    let leases = f.ctx.extensions().get::<AuthoringLeases>().unwrap();
    let lease = leases.hold();
    fail(2);
    let held = f.ctx.authoring().is_holder(&holder);
    assert!(held, "released under a lease");
    drop(lease);
}

#[test]
fn a_pressure_session_neither_ends_at_grace_expiry_nor_releases_authoring() {
    let f = Fixture::new(false);
    let (holder, ctx) = (browser(), Arc::clone(&f.ctx));
    f.connect(&holder);
    f.ctx.require_authoring(&holder, Take::TakeIfFree).unwrap();
    let guidance =
        Box::new(move |dir: &SessionDir| FixedGuidance.authoring(&ctx, dir, None, None, None));
    let request = StartRequest {
        kind: SessionKind::Pressure {
            plan: "doc/plans/PLAN-x.md".into(),
        },
        guidance,
        opening_origin: None,
        offer: None,
        feature: None,
        plan: None,
        extra_env: Vec::new(),
    };
    let s = f.launched(&f.bridge.start_session(request).unwrap());
    f.expire_grace(&holder);
    thread::sleep(Duration::from_millis(200));
    assert_eq!(f.state(&s), Some(SessionState::Starting), "grace ended it");
    f.connect(&holder);
    f.ctx.require_authoring(&holder, Take::TakeIfFree).unwrap();
    let ended = f.bridge.end(&s.id, EndMode::Teardown, EndCause::Caller);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("the end", || !f.ended(&s.id).is_empty());
    let held = f.ctx.authoring().is_holder(&holder);
    assert!(held, "a pressure end freed authoring");
}
