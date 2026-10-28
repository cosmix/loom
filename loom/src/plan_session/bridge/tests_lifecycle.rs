//! Bridge tests: a Stop that a running subagent holds open, abort during a launch, an end
//! nobody asked for and its queue, a spawn whose teardown failed, an oversized closing message.

use std::sync::Arc;
use std::thread;

use serde_json::json;

use super::fake_host::*;
use super::host::socket_name;
use super::registry::read_record;
use super::{EndCause, EndMode, EndOutcome, SendOutcome, TEARDOWN_FAILED};
use crate::plan_session::input::MAX_DELIVERED_BYTES;
use crate::plan_session::kind::SessionDir;
use crate::plan_session::kind::SessionKind::{Authoring, Summary};
use crate::plan_session::model::{ChatDelta, FailureKind, Item, SessionState};
use crate::process::IdentityStatus;

#[test]
fn a_summary_session_stays_open_at_a_stop_while_a_subagent_runs() {
    let h = Harness::ready(|_| {});
    let s = h.start(Summary);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": OPENING}));
    h.spool(&s, "SubagentStart", json!({"agent_id": "a1"}));
    h.spool(&s, "Stop", json!({}));
    settle();
    assert_eq!(
        h.ended(&s.id),
        None,
        "ended at a Stop with a subagent running"
    );
    h.spool(&s, "SubagentStop", json!({"agent_id": "a1"}));
    h.spool(&s, "Stop", json!({}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
}

#[test]
fn a_summary_session_ends_at_a_stop_failure() {
    let h = Harness::ready(|_| {});
    let s = h.start(Summary);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": OPENING}));
    h.spool(&s, "StopFailure", json!({"error": "server_error"}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
}

#[test]
fn abort_waits_for_a_launch_in_flight_and_tears_it_down() {
    let h = Harness::ready(|_| {});
    h.host.hold_spawn(true);
    let request = h.request(Authoring, OPENING, false);
    let id = h.bridge.start_session(request).unwrap();
    let socket = socket_name(&id);
    wait_until("the spawn", || {
        h.host.count(&Call::Spawn(socket.clone())) == 1
    });
    let bridge = h.bridge.clone();
    let aborting = thread::spawn(move || bridge.abort());
    wait_until("the stop", || h.bridge.lock().stopping);
    settle();
    assert!(
        !aborting.is_finished(),
        "abort returned while a launch was in flight"
    );
    h.host.hold_spawn(false);
    aborting.join().unwrap();
    assert_eq!(h.host.count(&Call::Teardown(socket.clone())), 1);
    assert!(!h.runtime.sessions.join(&id).exists());
    wait_until("the end", || h.ended(&id) == Some(EndOutcome::TornDown));
}

#[test]
fn a_session_that_dies_with_a_queue_clears_it_in_every_tab() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let sent = h.bridge.send(&s.id, "B".to_owned(), Vec::new());
    assert!(matches!(sent, Ok(SendOutcome::Queued { .. })), "{sent:?}");
    h.host.set_identity(s.pid, IdentityStatus::Dead);
    wait_until("the failure", || {
        matches!(h.ended(&s.id), Some(EndOutcome::Failed(_)))
    });
    let deltas = h.deltas.lock().unwrap();
    let queue = deltas.iter().rev().find_map(|delta| match delta {
        ChatDelta::Queue { queue } => Some(queue.len()),
        _ => None,
    });
    assert_eq!(queue, Some(0), "the tabs still show B queued");
    let view = deltas.iter().rev().find_map(|delta| match delta {
        ChatDelta::Session { session } => Some(session.queued),
        _ => None,
    });
    assert_eq!(view, Some(0));
}

#[test]
fn a_spawn_whose_teardown_failed_keeps_its_directory_and_record() {
    let h = Harness::ready(|_| {});
    h.host.fail_spawn_teardown();
    h.host.fail_teardown(true);
    let request = h.request(Authoring, OPENING, false);
    let id = h.bridge.start_session(request).unwrap();
    wait_until("the failure", || {
        matches!(h.ended(&id), Some(EndOutcome::Failed(_)))
    });
    let dir = SessionDir::new(h.runtime.sessions.join(&id));
    let record = read_record(&dir).unwrap();
    assert_eq!(record.pid, None, "not the provisional record");
    let sessions = h.bridge.sessions();
    let kept = sessions.iter().find(|(listed, ..)| *listed == id);
    assert_eq!(kept.map(|(.., state)| *state), Some(SessionState::Failed));
    let previous = h.bridge.chat_snapshot(false).previous.unwrap();
    let failure = previous.session.failure.unwrap();
    assert_eq!(failure.kind, FailureKind::Spawn);
    assert!(failure.message.starts_with(TEARDOWN_FAILED), "{failure:?}");
    let teardowns = |h: &Harness| h.host.count(&Call::Teardown(socket_name(&id)));
    wait_until("a retry", || teardowns(&h) >= 1);
    h.bridge
        .start_session(h.request(Authoring, OPENING, false))
        .unwrap();
    assert!(dir.record().is_file(), "the next start removed the record");
    h.host.fail_teardown(false);
    wait_until("the directory removed", || !dir.path().exists());
}

#[test]
fn a_closing_message_over_the_bound_is_not_pasted_and_the_session_is_torn_down() {
    let oversized = Arc::new(|| "x".repeat(MAX_DELIVERED_BYTES + 1));
    let h = Harness::ready(|c| c.closing_message = oversized);
    let s = h.idle(Authoring);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::TornDown));
    assert_eq!(h.sent(&s), Vec::<String>::new(), "the closing went out");
    let previous = h.bridge.chat_snapshot(false).previous.unwrap();
    let mut items = previous.turns.iter().flat_map(|turn| &turn.items);
    let said = |item: &Item| {
        let why = "The closing message cannot be sent";
        matches!(item, Item::Notice { text, .. } if text.starts_with(why))
    };
    assert!(items.any(said), "the chat was not told why");
}
