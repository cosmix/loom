//! Returned input: what waits for the browser that held authoring before a restart.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::install_with;
use super::returned::{load, on_returned, tag, Returned, RETURNED_FILE};
use super::tests::{browser, wait_until, FakeHost, Fixture};
use super::{lock, ChatState};
use crate::plan_server::authoring::{FreeReason, Take};
use crate::plan_server::context::{BindInfo, PlanContext};
use crate::plan_server::ids::BrowserId;
use crate::plan_server::prereq::ReadOnly;
use crate::plan_session::bridge::{take_unsaved, Bridge};
use crate::planning::runtime::{create_runtime_dir, runtime_paths_under};
use crate::planning::watch::OwnWrites;

const KEPT: &str = "keep this";

fn expected() -> Value {
    json!({"type": "chat", "frame": "queue_returned", "texts": [KEPT], "cause": "recovery"})
}

fn state_of(f: &Fixture) -> Arc<ChatState> {
    f.ctx.extensions().get::<ChatState>().unwrap()
}

/// `f` as a restarted server sees it: `holder` held authoring before the restart.
fn restarted(f: &Fixture, holder: &BrowserId) -> Arc<ChatState> {
    let state = state_of(f);
    *lock(&state.returned) = Returned {
        prior: Some(tag(holder)),
        ..Returned::default()
    };
    state
}

fn waiting(f: &Fixture) -> bool {
    f.ctx.runtime().dir.join(RETURNED_FILE).exists()
}

#[test]
fn only_the_prior_holders_new_tab_gets_the_returned_input() {
    let f = Fixture::new(false);
    let (holder, other) = (browser(), browser());
    let state = restarted(&f, &holder);
    on_returned(&f.ctx, &state, vec![KEPT.to_owned()]);
    assert!(waiting(&f), "the input was not saved");

    let bystander = f.connect(&other);
    assert!(
        !bystander
            .opened
            .iter()
            .any(|f| f["frame"] == "queue_returned"),
        "another browser got it: {:?}",
        bystander.opened
    );
    assert!(waiting(&f), "another browser's tab took it");

    let returning = f.connect(&holder);
    assert!(
        returning.opened.contains(&expected()),
        "{:?}",
        returning.opened
    );
    assert!(!waiting(&f), "the delivered input stayed on disk");
    let again = f.connect(&holder);
    assert!(!again.opened.iter().any(|f| f["frame"] == "queue_returned"));
}

#[test]
fn an_open_holder_tab_gets_the_returned_input_at_once() {
    let f = Fixture::new(false);
    let holder = browser();
    let state = restarted(&f, &holder);
    let tab = f.connect(&holder);
    assert!(!tab.opened.iter().any(|f| f["frame"] == "queue_returned"));

    on_returned(&f.ctx, &state, vec![KEPT.to_owned()]);

    wait_until("the returned frame", || tab.frames().contains(&expected()));
    assert!(!waiting(&f), "the delivered input stayed on disk");
}

/// A chat that ended Failed frees authoring while its queued input is still to come back; a
/// restart before teardown must still name that browser, so another browser's first tab gets
/// nothing.
#[test]
fn a_freed_holders_returned_input_goes_only_to_that_holders_browser() {
    let f = Fixture::new(false);
    let (holder, other) = (browser(), browser());
    let tab = f.connect(&holder);
    f.ctx
        .authoring()
        .require(&holder, &tab.conn.public_id, Take::TakeIfFree, false)
        .unwrap();
    f.ctx
        .authoring()
        .release(FreeReason::Released("the chat ended"));
    assert!(f.ctx.authoring().holder().is_none());

    let state = state_of(&f);
    *lock(&state.returned) = load(&f.ctx);
    on_returned(&f.ctx, &state, vec![KEPT.to_owned()]);

    let bystander = f.connect(&other);
    assert!(
        !bystander
            .opened
            .iter()
            .any(|f| f["frame"] == "queue_returned"),
        "another browser got the freed holder's input: {:?}",
        bystander.opened
    );
    assert!(waiting(&f), "another browser's tab took it");
    let returning = f.connect(&holder);
    assert!(
        returning.opened.contains(&expected()),
        "{:?}",
        returning.opened
    );
}

/// Recovery removes its own copy of the input once `on_returned` returns: a first write that
/// failed must tell it to keep that copy.
#[test]
fn an_input_that_cannot_be_saved_is_reported_to_recovery() {
    let f = Fixture::new(false);
    let state = restarted(&f, &browser());
    // A directory in the file's place makes the replace fail.
    std::fs::create_dir(f.ctx.runtime().dir.join(RETURNED_FILE)).unwrap();
    assert!(!take_unsaved());

    on_returned(&f.ctx, &state, vec![KEPT.to_owned()]);
    assert!(take_unsaved(), "the failed write went unreported");

    std::fs::remove_dir(f.ctx.runtime().dir.join(RETURNED_FILE)).unwrap();
    on_returned(&f.ctx, &state, vec![KEPT.to_owned()]);
    assert!(!take_unsaved(), "a saved input was reported unsaved");
}

/// A context that shutdown dropped can no longer save the input: the listener must say so,
/// or recovery removes the session's own copy and the text is lost.
#[test]
fn input_returned_after_the_context_dropped_is_reported_unsaved() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    std::fs::create_dir_all(root.join("doc/product")).unwrap();
    let local = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7272);
    let bind = BindInfo::new(local, &"ab".repeat(32));
    let runtime = runtime_paths_under(&tmp.path().join("loom-home"), &root).unwrap();
    create_runtime_dir(&runtime).unwrap();
    let ctx = PlanContext::new(
        root.clone(),
        ReadOnly::from_problems(vec![]),
        bind,
        OwnWrites::default(),
    );
    let ctx = Arc::new(ctx.unwrap().with_runtime(runtime.clone()));
    install_with(&ctx, Arc::new(FakeHost::default()), move |config| {
        config.runtime = runtime;
        config.home = root.join("home");
        config.tick = Duration::from_millis(20);
    });
    let bridge = ctx.extensions().get::<Bridge>().unwrap();
    let (tx, rx) = mpsc::channel();
    bridge.on_recovered(Box::new(move |_| tx.send(()).unwrap_or_default()));
    rx.recv_timeout(Duration::from_secs(5)).expect("recovery");

    let kept = bridge.return_input("session-1", vec![KEPT.to_owned()]);
    assert_eq!(kept, Ok(()), "a live context saves the input");
    drop(ctx);
    let lost = bridge.return_input("session-1", vec![KEPT.to_owned()]);
    assert!(
        lost.is_err(),
        "the dropped context's input counted as saved"
    );
}
