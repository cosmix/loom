//! Bridge tests: final writes, summary and pressure ends, subagents holding an end, failed
//! teardowns, abort and the marker release.

use std::fs;
use std::io::Write;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use serde_json::json;

use super::fake_host::*;
use super::fixture_replay::tasks;
use super::{BridgeError, BridgeEvent, EndCause, EndMode, EndOutcome};
use crate::plan_session::kind::SessionKind;
use crate::plan_session::kind::SessionKind::Authoring;
use crate::plan_session::model::{DenyReason, FailureKind, Item, SessionState};
use crate::process::IdentityStatus;

fn denial(h: &Harness, request: &str) -> Option<DenyReason> {
    let previous = h.bridge.chat_snapshot(false).previous?;
    let items = previous.turns.into_iter().flat_map(|turn| turn.items);
    let mut reasons = items.filter_map(|item| match item {
        Item::Permission {
            request_id,
            deny_reason,
            ..
        } if request_id == request => deny_reason,
        _ => None,
    });
    reasons.next()
}

fn started(h: &Harness, id: &str) -> bool {
    h.count_events(|e| matches!(e, BridgeEvent::Started { id: started, .. } if started == id)) == 1
}

#[test]
fn an_end_before_the_opening_tears_down_at_once_without_a_closing_message() {
    let h = Harness::ready(|_| {});
    let s = h.start(Authoring);
    wait_until("the start", || started(&h, &s.id));
    let asked = Instant::now();
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::TornDown));
    assert!(asked.elapsed() < Duration::from_secs(3));
    settle();
    assert_eq!(h.host.delivered(&s.socket), Vec::<String>::new());
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
    assert!(!s.dir.path().exists());
}

#[test]
fn two_subagent_starts_in_one_millisecond_both_count() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let line = |agent: &str| {
        let payload = json!({"session_id": s.id, "agent_id": agent});
        let event = json!({"received_at": at, "event": "SubagentStart", "payload": payload});
        format!("{event}\n")
    };
    // The repeated first line stands for a replayed spool: it must not count twice.
    let lines = [line("a1"), line("a2"), line("a1")].concat();
    let mut spool = fs::OpenOptions::new().append(true).open(s.dir.events());
    spool.as_mut().unwrap().write_all(lines.as_bytes()).unwrap();
    let running = |h: &Harness| {
        let ids = tasks(h, &s).into_iter().map(|task| task.id);
        ids.collect::<Vec<_>>()
    };
    wait_until("both subagents", || running(&h) == ["a1", "a2"]);
    settle();
    assert_eq!(running(&h), ["a1", "a2"], "the repeated line counted again");
}

#[test]
fn a_failed_record_and_teardown_after_the_spawn_keep_the_only_handle() {
    let h = Harness::ready(|_| {});
    h.host.fail_record();
    h.host.fail_teardown(true);
    let s = h.start(Authoring);
    wait_until("the failure", || {
        matches!(h.ended(&s.id), Some(EndOutcome::Failed(_)))
    });
    let teardowns = |h: &Harness| h.host.count(&Call::Teardown(s.socket.clone()));
    wait_until("a retry", || teardowns(&h) >= 2);
    assert_eq!(h.state(&s), Some(SessionState::Failed));
    let previous = h.bridge.chat_snapshot(false).previous;
    let failure = previous.and_then(|previous| previous.session.failure);
    assert_eq!(failure.map(|f| f.kind), Some(FailureKind::Spawn));
    let next = h.request(Authoring, OPENING, false);
    h.bridge.start_session(next).unwrap();
    assert!(
        s.dir.record().is_file(),
        "the next start removed the record"
    );
    h.host.fail_teardown(false);
    wait_until("the directory removed", || !s.dir.path().exists());
}

#[test]
fn a_summary_session_ends_at_its_first_stop() {
    let h = Harness::ready(|_| {});
    let s = h.start(SessionKind::Summary);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": OPENING}));
    h.spool(&s, "Stop", json!({}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
    assert!(!s.dir.path().exists());
    assert!(
        h.bridge.chat_snapshot(false).previous.is_none(),
        "a summary showed in the chat"
    );
}

#[test]
fn a_final_write_pastes_the_closing_message_at_idle_and_ends_after_its_stop() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    h.wait_state(&s, SessionState::Ending);
    settle();
    assert_eq!(h.sent(&s), Vec::<String>::new(), "pasted mid-turn");
    h.spool(&s, "Stop", json!({}));
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    settle();
    assert!(
        h.ended(&s.id).is_none(),
        "ended before the closing turn's Stop"
    );
    h.spool(&s, "Stop", json!({}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
}

#[test]
fn a_session_opened_by_the_users_typed_message_gets_the_closing_message_at_idle() {
    let h = Harness::ready(|_| {});
    // The harness opening carries no origin: it is the user's own first message.
    let s = h.idle(Authoring);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
}

#[test]
fn an_interrupt_keeps_the_subagents_so_the_final_write_waits_for_their_end() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    for agent in ["a1", "a2"] {
        h.spool(&s, "SubagentStart", json!({"agent_id": agent}));
    }
    h.bridge.interrupt(&s.id).unwrap();
    h.record(&s, interrupted());
    h.wait_state(&s, SessionState::Idle);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the closing message", || h.sent(&s) == [CLOSING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": CLOSING}));
    h.spool(&s, "Stop", json!({}));
    settle();
    assert_eq!(h.ended(&s.id), None, "closed while its agents ran");
    for agent in ["a1", "a2"] {
        h.spool(&s, "SubagentStop", json!({"agent_id": agent}));
    }
    h.spool(&s, "Stop", json!({}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
}

#[test]
fn a_final_write_ends_at_the_bound_without_a_stop() {
    let h = Harness::ready(|c| c.final_write_bound = Duration::from_millis(800));
    let s = h.idle(Authoring);
    let asked = Instant::now();
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::TornDown));
    assert!(asked.elapsed() < Duration::from_secs(3));
    assert_eq!(h.sent(&s), [CLOSING]);
}

#[test]
fn a_teardown_end_denies_what_is_pending_and_tears_down_now() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let request = h.permission(&s, "Bash", json!({"command": "ls"}));
    h.wait_state(&s, SessionState::Permission);
    h.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::TornDown));
    assert_eq!(denial(&h, &request), Some(DenyReason::SessionEnded));
    assert!(!s.dir.path().exists());
    assert_eq!(h.sent(&s), Vec::<String>::new());
}

#[test]
fn a_marker_end_keeps_the_directory_until_release() {
    let h = Harness::ready(|_| {});
    let s = h.idle(pressure());
    fs::create_dir_all(s.dir.done()).unwrap();
    fs::write(s.dir.marker(), b"").unwrap();
    wait_until("the marker end", || {
        h.ended(&s.id) == Some(EndOutcome::Marker)
    });
    assert!(s.dir.path().is_dir());
    assert_eq!(h.sent(&s), Vec::<String>::new());
    let early = h.bridge.release_session("no-such-session");
    assert!(matches!(early, Err(BridgeError::NoSession)), "{early:?}");
    h.bridge.release_session(&s.id).unwrap();
    assert!(!s.dir.path().exists());
}

#[test]
fn a_failed_teardown_is_failed_keeps_its_files_and_retries_while_alive() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.host.fail_teardown(true);
    h.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    let teardowns = |h: &Harness| h.host.count(&Call::Teardown(s.socket.clone()));
    wait_until("a retry", || teardowns(&h) >= 2);
    assert_eq!(h.state(&s), Some(SessionState::Failed));
    let failure = h
        .bridge
        .chat_snapshot(false)
        .previous
        .unwrap()
        .session
        .failure
        .unwrap();
    assert!(
        failure.message.starts_with("teardown failed"),
        "{failure:?}"
    );
    assert!(s.dir.record().is_file());
    let failed = h.count_events(|e| matches!(e, BridgeEvent::Ended { .. }));
    assert_eq!(failed, 1, "the failure was reported more than once");
    h.start(Authoring);
    h.host.fail_teardown(false);
    wait_until("the directory removed", || !s.dir.path().exists());
}

#[test]
fn a_dead_process_stops_the_retries() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.host.fail_teardown(true);
    h.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    h.wait_state(&s, SessionState::Failed);
    h.host.set_identity(s.pid, IdentityStatus::Dead);
    settle();
    let teardowns = h.host.count(&Call::Teardown(s.socket.clone()));
    settle();
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), teardowns);
    assert!(s.dir.record().is_file());
}

#[test]
fn abort_tears_every_session_down_from_the_calling_thread() {
    let h = Harness::ready(|_| {});
    let chat = h.idle(Authoring);
    let summary = h.start(SessionKind::Summary);
    wait_until("the summary's start", || started(&h, &summary.id));
    let request = h.permission(&chat, "Bash", json!({"command": "ls"}));
    h.wait_state(&chat, SessionState::Permission);
    h.bridge.abort();
    for s in [&chat, &summary] {
        assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
        assert_eq!(h.ended(&s.id), Some(EndOutcome::TornDown));
        assert!(!s.dir.path().exists());
    }
    assert_eq!(denial(&h, &request), Some(DenyReason::ServerStopping));
    let refused = h.bridge.start_session(h.request(Authoring, OPENING, false));
    assert!(matches!(refused, Err(BridgeError::Busy(_))), "{refused:?}");
}

#[test]
fn a_process_gone_while_ending_closes_without_a_failure() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    h.bridge
        .end(&s.id, EndMode::FinalWrite, EndCause::Holder)
        .unwrap();
    h.host.set_identity(s.pid, IdentityStatus::Dead);
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::TornDown));
    assert_eq!(h.state(&s), Some(SessionState::Ended));
    let failure = h
        .bridge
        .chat_snapshot(false)
        .previous
        .unwrap()
        .session
        .failure;
    assert_eq!(failure, None);
}

#[test]
fn a_session_end_hook_ends_the_session() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.spool(&s, "SessionEnd", json!({"reason": "other"}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::TornDown));
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
    assert!(!s.dir.path().exists());
}

#[test]
fn a_failed_opening_paste_fails_the_session_and_a_later_end_pastes_nothing() {
    let h = Harness::ready(|_| {});
    h.host.fail_deliver();
    let s = h.start(Authoring);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the failure", || {
        matches!(h.ended(&s.id), Some(EndOutcome::Failed(_)))
    });
    let previous = h.bridge.chat_snapshot(false).previous;
    let failure = previous.and_then(|previous| previous.session.failure);
    assert_eq!(failure.map(|f| f.kind), Some(FailureKind::Spawn));
    let end = h.bridge.end(&s.id, EndMode::FinalWrite, EndCause::Holder);
    assert!(end.is_err(), "{end:?}");
    settle();
    assert_eq!(h.host.delivered(&s.socket), [OPENING]);
    assert_eq!(h.host.count(&Call::Teardown(s.socket.clone())), 1);
    let ended = |e: &BridgeEvent| matches!(e, BridgeEvent::Ended { id, .. } if *id == s.id);
    assert_eq!(h.count_events(ended), 1);
    let next = h.request(Authoring, OPENING, false);
    h.bridge.start_session(next).unwrap();
    assert!(
        !s.dir.path().exists(),
        "the next start kept the failed directory"
    );
}

#[test]
fn a_failed_spawn_fails_the_session_and_the_next_start_clears_its_directory() {
    let h = Harness::ready(|_| {});
    h.host.fail_spawn();
    let request = h.request(Authoring, OPENING, false);
    let id = h.bridge.start_session(request).unwrap();
    wait_until("the failure", || {
        matches!(h.ended(&id), Some(EndOutcome::Failed(_)))
    });
    let previous = h.bridge.chat_snapshot(false).previous;
    let failure = previous.and_then(|previous| previous.session.failure);
    assert_eq!(failure.map(|f| f.kind), Some(FailureKind::Spawn));
    let dir = h.runtime.sessions.join(&id);
    assert!(dir.is_dir(), "the failed session's directory went");
    let next = h.request(Authoring, OPENING, false);
    h.bridge.start_session(next).unwrap();
    assert!(!dir.exists(), "the next start kept the failed directory");
}
