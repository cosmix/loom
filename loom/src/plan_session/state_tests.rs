use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};

use super::*;

pub(super) fn at(millis: i64) -> DateTime<Utc> {
    let start: DateTime<Utc> = "2026-10-27T10:00:00Z".parse().unwrap();
    start + Duration::milliseconds(millis)
}

pub(super) fn event(name: &str, millis: i64, payload: Value) -> SpoolEvent {
    SpoolEvent {
        received_at: at(millis),
        event: name.to_string(),
        session_id: Some("s".into()),
        transcript_path: None,
        request_id: None,
        payload,
    }
}

pub(super) fn request(id: &str, millis: i64) -> SpoolEvent {
    let mut request = event("PermissionRequest", millis, json!({"tool_name": "Bash"}));
    request.request_id = Some(id.to_string());
    request
}

pub(super) fn feed(machine: &mut StateMachine, event: SpoolEvent) -> Option<SessionState> {
    machine.apply(Input::Spool(&event))
}

pub(super) fn start(machine: &mut StateMachine, source: &str, millis: i64) -> Option<SessionState> {
    feed(
        machine,
        event("SessionStart", millis, json!({"source": source})),
    )
}

fn idle() -> StateMachine {
    let mut machine = StateMachine::new();
    start(&mut machine, "startup", 0);
    machine
}

pub(super) fn working() -> StateMachine {
    let mut machine = idle();
    feed(
        &mut machine,
        event("UserPromptSubmit", 100, json!({"prompt": "go"})),
    );
    assert_eq!(machine.state(), SessionState::Working);
    machine
}

pub(super) fn stop(millis: i64) -> SpoolEvent {
    event("Stop", millis, json!({"stop_hook_active": false}))
}

fn main_record(millis: i64) -> Input<'static> {
    Input::Record {
        at: at(millis),
        main: true,
    }
}

fn ids(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn a_spawned_session_is_starting() {
    let machine = StateMachine::new();
    assert_eq!(machine.state(), SessionState::Starting);
    assert_eq!(machine.subagents_running(), 0);
    assert!(machine.pending_requests().is_empty());
}

#[test]
fn session_start_startup_moves_starting_to_idle() {
    let mut machine = StateMachine::new();
    assert_eq!(start(&mut machine, "startup", 0), Some(SessionState::Idle));
    assert_eq!(machine.last_idle_at(), Some(at(0)));
}

#[test]
fn a_session_start_without_a_source_moves_nothing() {
    let mut machine = StateMachine::new();
    assert_eq!(
        feed(&mut machine, event("SessionStart", 0, json!({}))),
        None
    );
    assert_eq!(machine.state(), SessionState::Starting);
}

#[test]
fn user_prompt_submit_moves_idle_to_working() {
    let mut machine = idle();
    let changed = feed(&mut machine, event("UserPromptSubmit", 100, json!({})));
    assert_eq!(changed, Some(SessionState::Working));
}

#[test]
fn a_main_stop_moves_working_to_idle() {
    let mut machine = working();
    assert_eq!(feed(&mut machine, stop(1_000)), Some(SessionState::Idle));
}

#[test]
fn an_interrupt_record_moves_working_to_idle() {
    let mut machine = working();
    let changed = machine.apply(Input::Interrupted { at: at(1_000) });
    assert_eq!(changed, Some(SessionState::Idle));
}

#[test]
fn an_idle_prompt_notification_moves_working_to_idle() {
    let mut machine = working();
    let idle = json!({"notification_type": "idle_prompt", "message": "Claude is waiting"});
    assert_eq!(
        feed(&mut machine, event("Notification", 1_000, idle)),
        Some(SessionState::Idle)
    );

    let mut untyped = working();
    let message = json!({"message": "Claude is waiting for your input"});
    feed(&mut untyped, event("Notification", 1_000, message));
    assert_eq!(untyped.state(), SessionState::Idle, "the message fallback");

    let mut other = working();
    let prompt = json!({"notification_type": "permission_prompt", "message": "needs approval"});
    assert_eq!(feed(&mut other, event("Notification", 1_000, prompt)), None);
}

#[test]
fn a_newer_main_record_moves_idle_to_working() {
    let mut machine = working();
    feed(&mut machine, stop(1_000));
    assert_eq!(
        machine.apply(main_record(2_000)),
        Some(SessionState::Working)
    );
    assert_eq!(machine.last_main_at(), Some(at(2_000)));
}

#[test]
fn a_main_record_older_than_the_stop_keeps_idle() {
    let mut machine = working();
    feed(&mut machine, stop(1_000));
    assert_eq!(machine.apply(main_record(900)), None);
    assert_eq!(machine.state(), SessionState::Idle);
}

#[test]
fn idle_evidence_received_before_a_later_record_does_not_win() {
    let mut machine = working();
    feed(&mut machine, stop(1_000));
    machine.apply(main_record(2_000));
    let idle = json!({"notification_type": "idle_prompt"});
    assert_eq!(feed(&mut machine, event("Notification", 1_500, idle)), None);
    assert_eq!(feed(&mut machine, stop(1_800)), None);
    assert_eq!(machine.state(), SessionState::Working);
}

#[test]
fn a_subagent_record_after_stop_stays_idle() {
    let mut machine = working();
    feed(&mut machine, stop(1_000));
    let record = Input::Record {
        at: at(2_000),
        main: false,
    };
    assert_eq!(machine.apply(record), None);
    assert_eq!(machine.state(), SessionState::Idle);
}

#[test]
fn permission_request_and_answer_move_working_to_permission_and_back() {
    let mut machine = working();
    assert_eq!(
        feed(&mut machine, request("r1", 1_000)),
        Some(SessionState::Permission)
    );
    let answered = machine.apply(Input::PermissionAnswered {
        request_id: "r1".into(),
    });
    assert_eq!(answered, Some(SessionState::Working));
}

#[test]
fn two_pending_requests_keep_permission_until_each_id_is_removed() {
    let removals: [fn(String) -> Input<'static>; 3] = [
        |request_id| Input::PermissionAnswered { request_id },
        |request_id| Input::PermissionExpired { request_id },
        |request_id| Input::RequestResolved { request_id },
    ];
    for remove in removals {
        let mut machine = working();
        feed(&mut machine, request("r1", 1_000));
        feed(&mut machine, request("r2", 1_100));
        assert_eq!(machine.pending_requests(), &ids(&["r1", "r2"]));
        assert_eq!(machine.apply(remove("r1".into())), None);
        assert_eq!(machine.pending_requests(), &ids(&["r2"]));
        assert_eq!(machine.state(), SessionState::Permission);
        assert_eq!(
            machine.apply(remove("r2".into())),
            Some(SessionState::Working)
        );
    }
}

#[test]
fn an_answer_for_an_unknown_id_changes_nothing() {
    let mut machine = idle();
    let changed = machine.apply(Input::PermissionAnswered {
        request_id: "r9".into(),
    });
    assert_eq!((changed, machine.state()), (None, SessionState::Idle));
    let expired = machine.apply(Input::PermissionExpired {
        request_id: "r9".into(),
    });
    assert_eq!(expired, None);
}

#[test]
fn an_auto_denied_request_is_never_pending() {
    let mut machine = idle();
    let mut denied = request("r1", 1_000);
    denied.payload["auto_denied"] = json!("summary_session");
    assert_eq!(feed(&mut machine, denied), None);
    assert!(machine.pending_requests().is_empty());
}

#[test]
fn a_stop_or_an_interrupt_after_the_newest_request_clears_the_set() {
    let mut stopped = working();
    feed(&mut stopped, request("r1", 1_000));
    feed(&mut stopped, request("r2", 1_100));
    assert_eq!(feed(&mut stopped, stop(3_100)), Some(SessionState::Idle));
    assert!(stopped.pending_requests().is_empty());

    let mut interrupted = working();
    feed(&mut interrupted, request("r1", 1_000));
    let changed = interrupted.apply(Input::Interrupted { at: at(3_000) });
    assert_eq!(changed, Some(SessionState::Idle));
    assert!(interrupted.pending_requests().is_empty());
}

#[test]
fn a_main_record_after_a_request_keeps_it_pending() {
    // Another tool's result (px-state-R03) never names the request.
    let mut machine = working();
    feed(&mut machine, request("r1", 1_000));
    assert_eq!(machine.apply(main_record(3_000)), None);
    assert_eq!(machine.pending_requests(), &ids(&["r1"]));
    assert_eq!(machine.state(), SessionState::Permission);
}

#[test]
fn a_self_started_turn_moves_idle_to_working() {
    let mut machine = working();
    feed(&mut machine, stop(1_000));
    let changed = machine.apply(Input::SelfStartedTurn { at: at(2_000) });
    assert_eq!(changed, Some(SessionState::Working));
}

#[test]
fn a_delivered_compact_then_session_start_compact_is_idle() {
    let mut machine = working();
    machine.apply(Input::CompactDelivered);
    feed(
        &mut machine,
        event("UserPromptSubmit", 1_000, json!({"prompt": "/compact"})),
    );
    assert_eq!(
        start(&mut machine, "compact", 2_000),
        Some(SessionState::Idle)
    );
    assert_eq!(machine.activity(), Activity::Idle);
}

#[test]
fn a_compact_source_without_a_delivered_compact_changes_nothing() {
    let mut machine = working();
    feed(&mut machine, subagent("SubagentStart", 500));
    assert_eq!(start(&mut machine, "compact", 1_000), None);
    assert_eq!(machine.state(), SessionState::Working);
    assert_eq!(machine.subagents_running(), 1);

    let mut resting = idle();
    assert_eq!(start(&mut resting, "compact", 1_000), None);
    assert_eq!(resting.state(), SessionState::Idle);
}

pub(super) fn subagent(name: &str, millis: i64) -> SpoolEvent {
    event(name, millis, json!({"agent_id": "a7f3"}))
}
