//! The hook binary and the permission answers: mismatched events, a vanished session directory,
//! summary sessions and answer-file ids.

use std::fs;
use std::time::Duration;

use loom::plan_session::hook_cli;
use loom::plan_session::kind::SessionKind;
use loom::plan_session::model::DenyReason;
use loom::plan_session::permission::{write_answer, Answer, Behavior};
use loom::plan_session::spool::SpoolReader;
use serde_json::{json, Value};

use super::support::*;

#[test]
fn hook_with_a_mismatched_event_name_appends_nothing() {
    let session = Session::new();
    for name in ["SubagentStop", "Stop"] {
        let payload = hook_payload(name, json!({"stop_hook_active": false}));
        let stdin = serde_json::to_vec(&payload).unwrap();
        let mut out = Vec::new();
        let code = hook_cli::run("Stop", &session.dir, stdin.as_slice(), &mut out);
        assert_eq!(code, 0, "{name}");
        assert!(
            out.is_empty(),
            "{name} printed {}",
            String::from_utf8_lossy(&out)
        );
    }

    let events = SpoolReader::new(session.events()).read_new();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].event, "Stop");
    assert_eq!(events[0].payload["hook_event_name"], "Stop");
}

#[test]
fn permission_hook_denies_when_the_session_directory_goes_away() {
    let session = Session::new();
    session.write_guard(SessionKind::Authoring);
    let done = session.spawn_hook("PermissionRequest", &bash_permission_payload());
    session.wait_for_spool_line();
    let line = &session.spool_lines()[0];
    assert_eq!(line["event"], "PermissionRequest", "{line}");
    assert!(done.try_recv().is_err(), "the hook must wait for an answer");

    fs::remove_dir_all(session.answers()).unwrap();
    let (code, out) = done
        .recv_timeout(Duration::from_secs(2))
        .expect("the hook answers within 2 s of answers/ going away");
    assert_eq!(code, 0);
    assert!(!deny_message(&out).is_empty());
}

#[test]
fn summary_session_permission_requests_are_denied_at_once_and_logged() {
    let session = Session::new();
    session.write_guard(SessionKind::Summary);
    let done = session.spawn_hook("PermissionRequest", &bash_permission_payload());
    let (code, out) = done
        .recv_timeout(Duration::from_secs(2))
        .expect("a summary session's request is answered at once");
    assert_eq!(code, 0);
    let message = "summary sessions have no one to ask; their rules are wrong if this appears";
    assert_eq!(deny_message(&out), message);

    let lines = session.spool_lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["event"], "PermissionRequest");
    assert_eq!(lines[0]["payload"]["tool_name"], "Bash");
    // The pinned line form names the key, not its nesting: top level or inside the payload.
    let marked = |line: &Value| line.get("auto_denied").map(|tag| tag == "summary_session");
    let mark = marked(&lines[0]).or_else(|| marked(&lines[0]["payload"]));
    assert_eq!(mark, Some(true), "{}", lines[0]);
}

#[test]
fn write_answer_refuses_a_request_id_that_is_not_a_uuid() {
    let session = Session::new();
    let answer = Answer {
        behavior: Behavior::Deny,
        message: Some("not now".into()),
        reason: Some(DenyReason::Holder),
    };
    let before = tree(&session.base);
    for id in ["../escape", "a/b", ""] {
        assert!(write_answer(&session.dir, id, &answer).is_err(), "{id:?}");
    }
    assert_eq!(tree(&session.base), before, "a refused id created a file");

    let id = uuid::Uuid::new_v4().to_string();
    write_answer(&session.dir, &id, &answer).unwrap();
    let names: Vec<String> = fs::read_dir(session.answers())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [format!("{id}.json")]);
    let bytes = fs::read(session.answers().join(format!("{id}.json"))).unwrap();
    let written: Value = serde_json::from_slice(&bytes).unwrap();
    let expected = json!({"behavior": "deny", "message": "not now", "reason": "holder"});
    assert_eq!(written, expected);
}
