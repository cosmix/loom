//! Contract of a restart's returned input: a message the last server accepted and never saw
//! confirmed comes back, after the restart, to the browser that held authoring, as one
//! `queue_returned` frame with the cause `recovery`. It is not pasted into the closing session,
//! it is not lost for a tab that opens after recovery ran, and it reaches no other browser.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::install_with;
use super::tests::{browser, ok_result, wait_until, FakeHost, Fixture};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::BrowserId;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_server::wire::chat::ChatCommand;
use crate::plan_session::bridge::Bridge;
use crate::plan_session::model::SessionState;
use crate::planning::watch::OwnWrites;

const KEPT: &str = "keep this";

/// A server started over the runtime directory the first one left, with its own host. Its
/// bridge is aborted when it drops.
struct Restarted {
    ctx: Arc<PlanContext>,
    host: Arc<FakeHost>,
    bridge: Arc<Bridge>,
}

impl Drop for Restarted {
    fn drop(&mut self) {
        self.bridge.abort();
    }
}

/// Install a second chat over `f`'s project and runtime and wait for its recovery to finish.
fn restart(f: &Fixture) -> Restarted {
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7273);
    let bind = BindInfo::new(local, &"ab".repeat(32));
    let read_only = ReadOnly::from_problems(Vec::new());
    let ctx = PlanContext::new(
        f.ctx.root().to_path_buf(),
        read_only,
        bind,
        OwnWrites::default(),
    );
    let runtime = f.ctx.runtime().clone();
    let ctx = Arc::new(ctx.unwrap().with_runtime(runtime.clone()));
    let host = Arc::new(FakeHost::default());
    let home = f.ctx.root().join("home");
    install_with(&ctx, host.clone(), move |config| {
        config.runtime = runtime;
        config.home = home;
        config.claude_bin = "/nonexistent/claude".into(); // FakeHost never runs it
        config.tick = Duration::from_millis(20);
        config.identity_interval = Duration::from_millis(100);
        config.final_write_bound = Duration::from_secs(1);
    });
    let bridge = ctx.extensions().get::<Bridge>().unwrap();
    let (tx, rx) = mpsc::channel();
    bridge.on_recovered(Box::new(move |_| tx.send(()).unwrap_or_default()));
    rx.recv_timeout(Duration::from_secs(15)).expect("recovery");
    Restarted { ctx, host, bridge }
}

/// One tab opened on the second context: the frames it opened with and those that follow.
struct Tab {
    seen: Vec<Value>,
    rx: mpsc::Receiver<Arc<String>>,
}

impl Tab {
    fn open(ctx: &PlanContext, browser: &BrowserId) -> Tab {
        let conn = ctx.conn_ctx(browser.clone());
        let rx = ctx.hub().register(&conn);
        ctx.authoring().connection_opened(browser);
        let opened = ctx.hub().open_frames(&conn);
        let seen = opened
            .iter()
            .map(|frame| serde_json::from_str(frame).unwrap());
        Tab {
            seen: seen.collect(),
            rx,
        }
    }

    /// Every frame that reached the tab so far, those it opened with included.
    fn frames(&mut self) -> &[Value] {
        while let Ok(frame) = self.rx.try_recv() {
            self.seen.push(serde_json::from_str(&frame).unwrap());
        }
        &self.seen
    }

    fn returned(&mut self) -> Vec<Value> {
        let returned = self
            .frames()
            .iter()
            .filter(|f| f["frame"] == "queue_returned");
        returned.cloned().collect()
    }
}

#[test]
fn a_restart_returns_the_queued_send_to_the_holder_with_the_recovery_cause() {
    let f = Fixture::new(false);
    let (holder, other) = (browser(), browser());
    let tab = f.connect(&holder);
    let s = f.idle_chat(&tab);
    f.spool(&s, "UserPromptSubmit", json!({"prompt": "go on"}));
    f.wait_state(&s, SessionState::Working);
    let send = ChatCommand::Send {
        client_id: Some("c1".into()),
        text: KEPT.into(),
        attachments: Vec::new(),
    };
    assert_eq!(f.command(&tab, send), ok_result(), "the send was refused");
    wait_until("the queued send", || {
        f.bridge.chat_snapshot(true).queue.len() == 1
    });

    let second = restart(&f);
    // Both tabs open after recovery ran, the other browser's first.
    let (mut bystander, mut returning) = (
        Tab::open(&second.ctx, &other),
        Tab::open(&second.ctx, &holder),
    );
    let expected = json!({"type": "chat", "frame": "queue_returned",
        "texts": [KEPT], "cause": "recovery"});
    let deadline = Instant::now() + Duration::from_secs(10);
    while !returning.returned().contains(&expected) {
        let got = returning.returned();
        assert!(
            Instant::now() < deadline,
            "no returned frame for the holder: {got:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }

    assert_eq!(
        bystander.returned(),
        Vec::<Value>::new(),
        "another browser got it"
    );
    let pasted = second.host.delivered_to(&s.socket);
    assert!(
        !pasted.iter().any(|text| text.contains(KEPT)),
        "pasted: {pasted:?}"
    );
}
