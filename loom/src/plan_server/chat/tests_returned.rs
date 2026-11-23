//! Returned input: what waits for the browser that held authoring before a restart.

use std::sync::Arc;

use serde_json::{json, Value};

use super::returned::{load, on_returned, tag, Returned, RETURNED_FILE};
use super::tests::{browser, wait_until, Fixture};
use super::{lock, ChatState};
use crate::plan_server::authoring::{FreeReason, Take};
use crate::plan_server::ids::BrowserId;

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
