//! Bridge tests: a local command's barrier and settle against what else the session does, the
//! class check of a send, and the command cell and launch spec the catalogue owner reads.

use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use super::fake_host::{settle, wait_until, Harness, Sess};
use super::fixture_replay::fixture_record;
use super::tests_slash::{delivered, info_notices, listed, send, turns_started, users};
use super::{BridgeError, EndCause, EndMode, SendOutcome};
use crate::plan_session::commands::CommandTable;
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{Delivery, SessionState};

/// The two `system` `local_command` records `/context` writes, in order.
fn context_records(s: &Sess) -> Vec<Value> {
    let local = |record: &Value| record["subtype"] == "local_command";
    let named = |r: &Value| local(r) && r.get("commandRun").is_none();
    let output = |r: &Value| local(r) && r.get("commandRun").is_some();
    let fixture = "p2-usage-context";
    vec![
        fixture_record(fixture, s, named),
        fixture_record(fixture, s, output),
    ]
}

fn context_shown(h: &Harness) -> bool {
    info_notices(h)
        .iter()
        .any(|text| text.contains("Context Usage"))
}

#[test]
fn context_output_after_the_window_confirms_it_late_with_its_notice() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400));
    let s = h.idle(Authoring);
    assert!(matches!(
        send(&h, &s, "/context"),
        SendOutcome::Delivering { .. }
    ));
    let undelivered = [(Delivery::Undelivered, None)];
    wait_until("timed out", || users(&h, "/context") == undelivered);
    for record in context_records(&s) {
        h.record(&s, record);
    }
    wait_until("confirmed late", || {
        delivered(&h, "/context") && context_shown(&h)
    });
    settle();
    assert_eq!(h.sent(&s), ["/context"], "the late paste was resent");
}

#[test]
fn a_late_local_confirmation_settles_and_releases_the_paused_queue_without_an_idle() {
    let h = Harness::ready(|c| c.delivery_timeout = Duration::from_millis(400));
    let s = h.idle(Authoring);
    send(&h, &s, "/context");
    let undelivered = [(Delivery::Undelivered, None)];
    wait_until("timed out", || users(&h, "/context") == undelivered);
    let queued = send(&h, &s, "after the context");
    assert!(matches!(queued, SendOutcome::Queued { .. }), "{queued:?}");
    for record in context_records(&s) {
        h.record(&s, record);
    }
    wait_until("confirmed late", || delivered(&h, "/context"));
    settle();
    assert_eq!(
        h.sent(&s),
        ["/context"],
        "pasted before the late one settled"
    );
    wait_until("the queued message pasted after the settle", || {
        h.sent(&s) == ["/context", "after the context"]
    });
}

#[test]
fn a_task_notification_during_a_fork_settle_cancels_its_idle_and_opens_the_main_turn() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    listed(&h, &s, &[]);
    assert!(matches!(
        send(&h, &s, "/probe-fork"),
        SendOutcome::Delivering { .. }
    ));
    wait_until("pasted", || h.sent(&s) == ["/probe-fork"]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": "/probe-fork"}));
    h.wait_state(&s, SessionState::Working);
    let fork = |r: &Value| {
        r["content"]
            .as_str()
            .is_some_and(|c| c.contains("<forked-skill-launch>"))
    };
    h.record(&s, fixture_record("p2-skills", &s, fork));
    let launched = || {
        info_notices(&h)
            .iter()
            .any(|t| t.starts_with("Running in the background"))
    };
    wait_until("the launch", launched);

    let before = turns_started(&h);
    let notified = |r: &Value| r["origin"]["kind"] == "task-notification";
    let notification = fixture_record("p2-skills", &s, notified);
    let prompt = notification["message"]["content"].clone();
    h.record(&s, notification);
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": prompt }));
    wait_until("the notification's turn", || {
        turns_started(&h) == before + 1
    });
    thread::sleep(Duration::from_millis(2500));
    let state = h.state(&s);
    assert_eq!(
        state,
        Some(SessionState::Working),
        "the settle closed the notification's turn"
    );
    h.spool(&s, "Stop", json!({}));
    h.wait_state(&s, SessionState::Idle);
}

#[test]
fn two_identical_context_commands_confirm_one_each_in_order() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    assert!(matches!(
        send(&h, &s, "/context"),
        SendOutcome::Delivering { .. }
    ));
    assert!(matches!(
        send(&h, &s, "/context"),
        SendOutcome::Queued { .. }
    ));
    wait_until("the first pasted", || h.sent(&s) == ["/context"]);
    for record in context_records(&s) {
        h.record(&s, record);
    }
    let first = [(Delivery::Delivered, None)];
    wait_until("the first confirmed", || users(&h, "/context") == first);
    settle();
    assert_eq!(
        h.sent(&s),
        ["/context"],
        "the second went in before the first settled"
    );
    wait_until("the second pasted", || h.sent(&s).len() == 2);
    let both = [(Delivery::Delivered, None), (Delivery::Sent, None)];
    assert_eq!(users(&h, "/context"), both);
    for record in context_records(&s) {
        h.record(&s, record);
    }
    let confirmed = [(Delivery::Delivered, None), (Delivery::Delivered, None)];
    wait_until("the second confirmed", || {
        users(&h, "/context") == confirmed
    });
}

#[test]
fn a_slash_send_the_table_refuses_now_is_never_pasted() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    let refused = |text: &str| match h.bridge.send(&s.id, text.to_owned(), Vec::new()) {
        Err(BridgeError::Refused(reason)) => reason,
        other => panic!("{text:?}: {other:?}"),
    };
    assert_eq!(refused("/clear"), "Changes the session outside the planner");
    assert_eq!(
        refused("\u{feff} /probe-personal x"),
        "Commands are still loading"
    );
    listed(&h, &s, &[]);
    assert_eq!(
        refused("/probe-unlisted-zz"),
        "Not in this session's command list"
    );
    settle();
    assert!(h.sent(&s).is_empty(), "{:?}", h.sent(&s));
    let skill = send(&h, &s, "/probe-personal x");
    assert!(matches!(skill, SendOutcome::Delivering { .. }), "{skill:?}");
}

#[test]
fn a_session_starts_on_the_loading_table_and_keeps_its_launch_spec_while_live() {
    let h = Harness::ready(|_| {});
    let s = h.start(Authoring);
    let cell = h.bridge.command_cell(&s.id).unwrap();
    assert_eq!(**cell.read().unwrap(), CommandTable::loading());
    wait_until("live", || h.bridge.launch_spec(&s.id).is_some());
    let (spawned, _) = h.host.spec(&s.id).unwrap();
    let kept = h.bridge.launch_spec(&s.id).unwrap();
    let launched = (spawned.id, spawned.socket, spawned.dir.path().to_owned());
    assert_eq!((kept.id, kept.socket, kept.dir.path().to_owned()), launched);
    let unknown = (h.bridge.command_cell("nope"), h.bridge.launch_spec("nope"));
    assert!(unknown.0.is_none() && unknown.1.is_none());
    let ended = h.bridge.end(&s.id, EndMode::Teardown, EndCause::Holder);
    assert!(ended.is_ok(), "{ended:?}");
    wait_until("ended", || h.ended(&s.id).is_some());
    assert!(
        h.bridge.launch_spec(&s.id).is_none(),
        "an ended session's spec"
    );
}
