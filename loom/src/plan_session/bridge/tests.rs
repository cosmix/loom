//! Bridge tests: start and launch, the opening, the one-chat rules, transcripts, turns and
//! their changed files, the snapshot and the listeners.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};

use super::driver::checked_transcript;
use super::fake_host::*;
use super::registry::read_record;
use super::{BridgeError, BridgeEvent, EndCause, EndMode, SendOutcome};
use crate::plan_session::kind::{SessionDir, SessionKind};
use crate::plan_session::model::{ChangedFile, ChatDelta, Item, SessionState};

fn texts(h: &Harness) -> Vec<String> {
    let turns = h.bridge.chat_snapshot(true).turns;
    let items = turns.into_iter().flat_map(|turn| turn.items);
    let texts = items.filter_map(|item| match item {
        Item::Text { markdown, .. } => Some(markdown),
        _ => None,
    });
    texts.collect()
}

fn text_record(text: &str) -> serde_json::Value {
    let message = uuid::Uuid::new_v4().to_string();
    json!({"type": "assistant", "message": {"id": message, "role": "assistant",
        "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}})
}

fn turns_ended(h: &Harness) -> Vec<Vec<ChangedFile>> {
    let events = h.events.lock().unwrap();
    let ended = events.iter().filter_map(|event| match event {
        BridgeEvent::TurnEnded { changed, .. } => Some(changed.clone()),
        _ => None,
    });
    ended.collect()
}

#[test]
fn the_record_is_on_disk_before_started_and_names_the_socket() {
    let h = Harness::ready(|_| {});
    let read: Arc<Mutex<Vec<Option<u32>>>> = Arc::default();
    let (sessions, seen) = (h.runtime.sessions.clone(), read.clone());
    h.bridge.subscribe(Box::new(move |event: &BridgeEvent| {
        if let BridgeEvent::Started { id, .. } = event {
            let record = read_record(&SessionDir::new(sessions.join(id)));
            seen.lock()
                .unwrap()
                .push(record.ok().and_then(|record| record.pid));
        }
    }));
    let s = h.start(SessionKind::Authoring);
    wait_until("Started", || !read.lock().unwrap().is_empty());
    assert_eq!(*read.lock().unwrap(), [Some(s.pid)]);
    assert_eq!(h.state(&s), Some(SessionState::Starting));
    let record = read_record(&s.dir).unwrap();
    assert_eq!(record.start_time, Some(42));
    assert_eq!(record.tmux_session, "plan-authoring");
    let hex = s.socket.strip_prefix("loom-plan-").unwrap();
    assert!(
        hex.len() == 12 && hex.chars().all(|c| c.is_ascii_hexdigit()),
        "{}",
        s.socket
    );
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    h.wait_state(&s, SessionState::Idle);
}

#[test]
fn the_opening_is_pasted_after_session_start_and_never_in_the_launch_spec() {
    let h = Harness::ready(|_| {});
    let s = h.start(SessionKind::Authoring);
    settle();
    assert!(
        h.host.delivered(&s.socket).is_empty(),
        "pasted before SessionStart"
    );
    let (spec, _) = h.host.spec(&s.id).unwrap();
    assert_eq!(spec.guidance.opening_message, "");
    assert_eq!(spec.opening_argv, None);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
}

#[test]
fn an_opening_on_argv_must_be_a_command_and_a_plan_path() {
    let h = Harness::ready(|_| {});
    let bad = h.request(pressure(), "read doc/plans/PLAN-x.md and more", true);
    let refused = h.bridge.start_session(bad);
    assert!(
        matches!(refused, Err(BridgeError::Refused(_))),
        "{refused:?}"
    );
    let opening = "/loom-plan:pressure doc/plans/PLAN-x.md";
    let id = h
        .bridge
        .start_session(h.request(pressure(), opening, true))
        .unwrap();
    let s = h.sess(&id);
    let (spec, _) = h.host.spec(&id).unwrap();
    assert_eq!(spec.opening_argv.as_deref(), Some(opening));
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    h.wait_state(&s, SessionState::Idle);
    settle();
    assert!(
        h.host.delivered(&s.socket).is_empty(),
        "an argv opening was pasted too"
    );
}

#[test]
fn one_chat_and_one_summary_session_at_a_time() {
    let h = Harness::ready(|_| {});
    h.start(SessionKind::Authoring);
    for kind in [SessionKind::Authoring, pressure()] {
        let refused = h.bridge.start_session(h.request(kind, OPENING, false));
        assert!(matches!(refused, Err(BridgeError::Busy(_))), "{refused:?}");
    }
    h.start(SessionKind::Summary);
    let again = h
        .bridge
        .start_session(h.request(SessionKind::Summary, OPENING, false));
    assert!(matches!(again, Err(BridgeError::Busy(_))), "{again:?}");
}

#[test]
fn a_new_transcript_path_is_followed_and_the_old_file_is_no_longer_read() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    let moved = h.runtime.dir.join("moved");
    fs::create_dir_all(&moved).unwrap();
    let second = moved.join(format!("{}.jsonl", s.id));
    fs::write(&second, b"").unwrap();
    let source = json!({"source": "resume", "transcript_path": second});
    h.spool(&s, "SessionStart", source);
    settle();
    h.working(&s);
    append_record(&second, text_record("from the new file"), &s.id);
    append_record(&s.transcript, text_record("from the old file"), &s.id);
    wait_until("the new file's text", || {
        texts(&h).contains(&"from the new file".to_owned())
    });
    settle();
    assert!(!texts(&h).contains(&"from the old file".to_owned()));
}

#[test]
fn a_file_changed_outside_the_tools_is_reported_with_its_pin() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    let pinned = vec![("doc/bash.md".to_owned(), Some("h1".to_owned()))];
    *h.pins.changes.lock().unwrap() = pinned;
    h.working(&s);
    let edit = json!({"file_path": "doc/edit.md"});
    h.record(
        &s,
        assistant_tools("msg_edit", &[("toolu_e", "Edit", edit)]),
    );
    let patch = json!({"filePath": "doc/edit.md", "structuredPatch": []});
    h.record(&s, tool_result("toolu_e", patch));
    settle();
    h.spool(&s, "Stop", json!({}));
    wait_until("the second TurnEnded", || turns_ended(&h).len() == 2);
    let bash = ChangedFile {
        path: "doc/bash.md".to_owned(),
        before_hash: Some("h1".to_owned()),
        by_tool: false,
    };
    let edited = ChangedFile {
        path: "doc/edit.md".to_owned(),
        before_hash: None,
        by_tool: true,
    };
    assert_eq!(turns_ended(&h)[1], [bash, edited]);
}

#[test]
fn a_listener_may_call_back_and_hears_deltas_in_order() {
    let h = Harness::ready(|_| {});
    let ops: Arc<Mutex<Vec<String>>> = Arc::default();
    let (bridge, seen) = (Arc::downgrade(&h.bridge), ops.clone());
    h.bridge.subscribe_chat(Box::new(move |delta: &ChatDelta| {
        if let Some(bridge) = bridge.upgrade() {
            bridge.chat_snapshot(true);
            bridge.sessions();
        }
        let op = serde_json::to_value(delta).unwrap()["op"]
            .as_str()
            .unwrap()
            .to_owned();
        seen.lock().unwrap().push(op);
    }));
    let s = h.idle(SessionKind::Authoring);
    let ops = ops.lock().unwrap().clone();
    assert_eq!(ops[..2], ["cleared", "session"], "{ops:?}");
    assert!(ops.contains(&"turn_added".to_owned()), "{ops:?}");
    assert_eq!(h.state(&s), Some(SessionState::Idle));
}

#[test]
fn an_ended_chat_stays_previous_until_the_next_start_clears_it() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.bridge
        .end(&s.id, EndMode::Teardown, EndCause::Caller)
        .unwrap();
    wait_until("the end", || h.ended(&s.id).is_some());
    let snapshot = h.bridge.chat_snapshot(false);
    assert!(snapshot.session.is_none());
    let previous = snapshot.previous.expect("no previous chat");
    assert_eq!(previous.session.id, s.id);
    assert!(!previous.turns.is_empty());
    let cleared = |h: &Harness| {
        let deltas = h.deltas.lock().unwrap();
        deltas
            .iter()
            .filter(|d| matches!(d, ChatDelta::Cleared))
            .count()
    };
    let before = cleared(&h);
    let next = h.start(SessionKind::Authoring);
    wait_until("Cleared", || cleared(&h) > before);
    let snapshot = h.bridge.chat_snapshot(false);
    assert!(snapshot.previous.is_none());
    assert_eq!(snapshot.session.map(|view| view.id), Some(next.id));
}

#[test]
fn a_task_notification_opens_a_turn_and_a_send_then_queues() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    let begun = h.pins.begins.lock().unwrap().len();
    let content = "<task-notification>\n<task-id>b1</task-id>";
    h.record(
        &s,
        json!({"type": "user", "message": {"role": "user", "content": content},
        "origin": {"kind": "task-notification"}}),
    );
    wait_until("TurnStarted", || {
        h.pins.begins.lock().unwrap().len() > begun
    });
    h.wait_state(&s, SessionState::Working);
    let sent = h.bridge.send(&s.id, "later".to_owned(), Vec::new());
    assert!(matches!(sent, Ok(SendOutcome::Queued { .. })), "{sent:?}");
}

#[test]
fn an_interrupt_record_ends_the_turn_once_and_a_later_stop_changes_nothing() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    let turn = h.pins.begins.lock().unwrap().last().cloned().unwrap();
    let before = turns_ended(&h).len();
    h.record(&s, interrupted());
    wait_until("TurnEnded", || turns_ended(&h).len() == before + 1);
    h.spool(&s, "Stop", json!({}));
    settle();
    assert_eq!(turns_ended(&h).len(), before + 1);
    let ends = h.pins.ends.lock().unwrap().clone();
    assert_eq!(ends.iter().filter(|key| **key == turn).count(), 1);
}

#[test]
fn a_token_transcript_line_adds_to_the_open_turn() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    h.working(&s);
    let extra = h.runtime.dir.join("teammate.jsonl");
    h.bridge.add_token_transcript(&s.id, extra.clone()).unwrap();
    let usage = json!({"type": "assistant", "message": {"id": "msg_team", "role": "assistant",
        "content": [], "usage": {"input_tokens": 100, "output_tokens": 7}}});
    append_record(&extra, usage, &s.id);
    wait_until("the teammate's tokens", || {
        let turns = h.bridge.chat_snapshot(true).turns;
        turns.last().is_some_and(|turn| turn.tokens.output >= 7)
    });
}

pub(super) fn append_record(path: &Path, record: Value, session: &str) {
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();
    writeln!(file, "{}", record_line(record, session)).unwrap();
}

/// `record` as one line of the session's transcript, stamped now.
pub(super) fn record_line(mut record: Value, session: &str) -> String {
    record["uuid"] = json!(uuid::Uuid::new_v4().to_string());
    record["parentUuid"] = Value::Null;
    record["isSidechain"] = json!(false);
    record["sessionId"] = json!(session);
    record["timestamp"] = json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
    record.to_string()
}

pub(super) fn assistant_tools(message: &str, tools: &[(&str, &str, Value)]) -> Value {
    let content: Vec<Value> = tools
        .iter()
        .map(
            |(id, name, input)| json!({"type": "tool_use", "id": id, "name": name, "input": input}),
        )
        .collect();
    json!({"type": "assistant", "message": {"id": message, "role": "assistant",
        "content": content, "stop_reason": "tool_use",
        "usage": {"input_tokens": 2, "output_tokens": 9}}})
}

pub(super) fn tool_result(id: &str, extra: Value) -> Value {
    json!({"type": "user", "message": {"role": "user", "content": [{"type": "tool_result",
        "tool_use_id": id, "content": "done", "is_error": false}]}, "toolUseResult": extra})
}

pub(super) fn interrupted() -> Value {
    json!({"type": "user", "message": {"role": "user",
        "content": [{"type": "text", "text": "[Request interrupted by user]"}]}})
}

/// Poll `f` every 10 ms for at most 5 s.
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

/// Long enough for the 20 ms driver to act on anything pending.
pub(super) fn settle() {
    thread::sleep(Duration::from_millis(300));
}

pub(super) fn pressure() -> SessionKind {
    SessionKind::Pressure {
        plan: "doc/plans/PLAN-tests.md".into(),
    }
}

#[test]
fn only_this_sessions_plain_absolute_transcript_path_is_tailed() {
    let id = "3f9a0c41-b7e2-4c1d-9a0e-1234567890ab";
    let ours = PathBuf::from(format!("/home/u/.claude/projects/p/{id}.jsonl"));
    assert_eq!(checked_transcript(&ours, id), Some(ours.clone()));
    let refused = [
        format!("relative/{id}.jsonl"),
        format!("/home/u/../../etc/{id}.jsonl"),
        "/home/u/.claude/projects/p/other.jsonl".to_owned(),
        format!("/home/u/{id}.json"),
    ];
    for path in refused {
        assert_eq!(checked_transcript(Path::new(&path), id), None, "{path}");
    }
}

#[test]
fn a_session_start_naming_another_transcript_is_ignored() {
    let h = Harness::ready(|_| {});
    let s = h.idle(SessionKind::Authoring);
    let foreign = h.transcript_of("other-session");
    fs::write(&foreign, b"").unwrap();
    let source = json!({"source": "resume", "transcript_path": foreign});
    h.spool(&s, "SessionStart", source);
    settle();
    h.working(&s);
    append_record(&foreign, text_record("foreign"), &s.id);
    h.record(&s, text_record("ours"));
    wait_until("our text", || texts(&h).contains(&"ours".to_owned()));
    settle();
    assert!(!texts(&h).contains(&"foreign".to_owned()));
}
