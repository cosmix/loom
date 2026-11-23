//! Contract: the queue a pressure start returns to the holder names its cause. The tab's
//! `queue_returned` frame carries the closed chat's texts and `"cause": "pressure"`. Rejects a
//! plain `queue_returned` frame with no cause.

use serde_json::json;
use serial_test::serial;

use super::{wait_until, Fixture};
use crate::plan_session::kind::SessionKind;

#[test]
#[serial]
fn a_pressure_start_returns_the_queue_with_the_pressure_cause() {
    let f = Fixture::new();
    let tab = f.connect(&f.holder);
    f.fake.list("chat-1", SessionKind::Authoring);
    *f.fake.end_texts.lock().unwrap() = vec!["one".into(), "two".into()];
    f.start(1);
    let returned = || {
        let frames = tab.frames();
        let found = frames
            .into_iter()
            .find(|frame| frame["frame"] == "queue_returned");
        found
    };
    wait_until("the returned queue", || returned().is_some());

    let frame = returned().unwrap();
    assert_eq!(frame["type"], "chat", "{frame}");
    assert_eq!(frame["texts"], json!(["one", "two"]), "{frame}");
    assert_eq!(frame["cause"], "pressure", "{frame}");
    f.finished();
}
