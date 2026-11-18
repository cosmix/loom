//! Authoring around a run: the open chat closes first, its queue returns to the holder, the
//! starter keeps authoring through every end, grace expiry stops the run, and the summary
//! suspension spans the run.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};
use serial_test::serial;

use super::fake::{Call, Script};
use super::{wait_until, Fixture};
use crate::plan_server::authoring::{Take, GRACE};
use crate::plan_server::chat::lease::AuthoringLeases;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::pressure::view::HOLDER_AWAY;
use crate::plan_server::wire::pressure::RunEndKind;
use crate::plan_session::bridge::{EndCause, EndMode};
use crate::plan_session::kind::SessionKind;

/// The holder's last tab closes and the grace period passes.
fn expire_grace(f: &Fixture) {
    let now = SystemTime::now();
    f.ctx.authoring().connection_closed(&f.holder, now);
    f.ctx.authoring().tick(now + GRACE + Duration::from_secs(1));
}

fn hold_first_step(f: &Fixture) {
    f.codex("sleep", "30");
    f.fake.script(vec![Script::Hold]);
}

#[test]
#[serial]
fn the_open_chat_ends_first_and_its_queue_returns_to_the_holder() {
    let f = Fixture::new();
    let tab = f.connect(&f.holder);
    f.fake.list("chat-1", SessionKind::Authoring);
    *f.fake.end_texts.lock().unwrap() = vec!["one".into(), "two".into()];
    *f.fake.end_delay.lock().unwrap() = Duration::from_millis(300);
    f.start(1);
    assert_eq!(f.ended().kind, RunEndKind::Recorded);
    let closing = Call::End("chat-1".into(), EndMode::FinalWrite, EndCause::Holder);
    assert_eq!(f.fake.calls().first(), Some(&closing));
    let chat_ended = {
        let ended = f.fake.ended.lock().unwrap();
        ended
            .iter()
            .find(|(id, ..)| id == "chat-1")
            .map(|(.., at)| *at)
            .unwrap()
    };
    let first_start = f.fake.started.lock().unwrap()[0].at;
    assert!(
        first_start >= chat_ended,
        "a step started before the chat ended"
    );
    wait_until("the queued texts", || !returned(&tab.frames()).is_empty());
    assert_eq!(returned(&tab.frames()), [json!(["one", "two"])]);
    let runs = tab
        .frames()
        .into_iter()
        .filter(|frame| frame["type"] == "pressure_run");
    let first = runs.into_iter().next().unwrap();
    assert_eq!(first["steps"], json!([]), "no closing view: {first}");
}

fn returned(frames: &[Value]) -> Vec<Value> {
    let returned = frames
        .iter()
        .filter(|frame| frame["frame"] == "queue_returned");
    returned.map(|frame| frame["texts"].clone()).collect()
}

#[test]
#[serial]
fn the_starter_keeps_authoring_through_the_chat_end_and_every_step_end() {
    let f = Fixture::new();
    f.fake.list("chat-1", SessionKind::Authoring);
    let seen: Arc<Mutex<Vec<(String, bool, bool)>>> = Arc::default();
    let (record, ctx) = (Arc::clone(&seen), Arc::downgrade(&f.ctx));
    let starter = f.ctx.presence().public_id(&f.holder);
    *f.fake.after_ended.lock().unwrap() = Some(Box::new(move |id: &str| {
        let ctx = ctx.upgrade().unwrap();
        let held = ctx.authoring().holder().map(|(holder, _)| holder) == Some(starter.clone());
        let lease = ctx.extensions().get::<AuthoringLeases>().unwrap().held();
        record.lock().unwrap().push((id.to_owned(), held, lease));
    }));
    f.start(2);
    assert_eq!(f.ended().kind, RunEndKind::Recorded);
    let seen = seen.lock().unwrap();
    let mid_run: Vec<&str> = seen.iter().map(|(id, ..)| id.as_str()).take(4).collect();
    assert_eq!(mid_run, ["chat-1", "s-1", "s-2", "s-3"]);
    assert!(
        seen.iter().take(4).all(|(_, held, lease)| *held && *lease),
        "{seen:?}"
    );
    assert_eq!(f.ctx.authoring().holder(), None);
    assert!(!f.ctx.extensions().get::<AuthoringLeases>().unwrap().held());
}

#[test]
#[serial]
fn grace_expiry_stops_the_run_without_a_record() {
    let f = Fixture::new();
    f.connect(&f.holder);
    hold_first_step(&f);
    f.start(2);
    wait_until("the first step", || {
        !f.fake.started.lock().unwrap().is_empty()
    });
    expire_grace(&f);
    let end = f.ended();
    assert_eq!(
        (end.kind, end.by.as_deref()),
        (RunEndKind::Stopped, Some(HOLDER_AWAY))
    );
    assert!(!f.plan_text().contains("## Pressure tests"));
}

#[test]
#[serial]
fn a_browser_taking_authoring_after_grace_expiry_keeps_it_at_the_run_end() {
    let f = Fixture::new();
    f.connect(&f.holder);
    hold_first_step(&f);
    let other = BrowserId::mint().unwrap();
    let (ctx, taker) = (Arc::downgrade(&f.ctx), other.clone());
    // The run's teardown runs inside the grace expiry; the other browser takes authoring then.
    *f.fake.on_end.lock().unwrap() = Some(Box::new(move |_: &str| {
        let ctx = ctx.upgrade().unwrap();
        ctx.require_authoring(&taker, Take::TakeIfFree).unwrap();
    }));
    f.start(1);
    wait_until("the first step", || {
        !f.fake.started.lock().unwrap().is_empty()
    });
    expire_grace(&f);
    assert_eq!(f.ended().kind, RunEndKind::Stopped);
    assert!(
        f.ctx.authoring().is_holder(&other),
        "the run's end released the new holder"
    );
}

#[test]
#[serial]
fn no_summary_session_starts_from_acceptance_to_the_run_end() {
    let f = Fixture::new();
    f.fake.list("chat-1", SessionKind::Authoring);
    let during_close: Arc<Mutex<Option<usize>>> = Arc::default();
    let (seen, rebuilds, summaries) = (
        Arc::clone(&during_close),
        Arc::clone(&f.rebuilds),
        Arc::clone(&f.summaries),
    );
    // The chat's closing turn ends while its write runs: no summary session may start then.
    *f.fake.on_end.lock().unwrap() = Some(Box::new(move |_: &str| {
        let ids = vec!["doc/product/index.md".to_owned()];
        rebuilds.enqueue(ids, crate::planning::summary::cost::Trigger::Button);
        let started = summaries.0.load(std::sync::atomic::Ordering::SeqCst);
        *seen.lock().unwrap() = Some(started);
    }));
    f.start(1);
    assert_eq!(
        f.press_rebuild(),
        0,
        "a summary session started during the run"
    );
    assert_eq!(f.ended().kind, RunEndKind::Recorded);
    assert_eq!(
        *during_close.lock().unwrap(),
        Some(0),
        "the closing write's end started one"
    );
    assert_eq!(f.press_rebuild(), 1, "the suspension outlived the run");
}
