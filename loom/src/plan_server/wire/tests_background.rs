//! Parity of the background frame with the web fixture.

use serde_json::Value;

use super::background::BackgroundTaskView;
use super::message::PlanMessage;
use crate::plan_session::bridge::background::{TaskKind, TaskState};

const FRAME: &str = include_str!("../../../../web/src/plan/api/fixtures/background.json");

fn parsed() -> (String, Vec<BackgroundTaskView>) {
    match serde_json::from_str(FRAME).unwrap() {
        PlanMessage::Background { session, tasks } => (session, tasks),
        other => panic!("not a background frame: {other:?}"),
    }
}

#[test]
fn the_fixture_is_the_serializers_text() {
    let message: PlanMessage = serde_json::from_str(FRAME).unwrap();
    let text = serde_json::to_string_pretty(&message).unwrap() + "\n";
    assert_eq!(text, FRAME);
}

#[test]
fn the_fixture_holds_the_five_tasks_of_the_chat_session() {
    let (session, tasks) = parsed();
    let snapshot: Value = serde_json::from_str(include_str!(
        "../../../../web/src/plan/api/fixtures/chat-snapshot.json"
    ))
    .unwrap();
    assert_eq!(snapshot["session"]["id"], session);
    let ids: Vec<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
    assert_eq!(ids, ["a-run", "bshell01", "a-quiet", "a-done", "bshell02"]);
    let unended = tasks.iter().filter(|task| !task.state.is_ended()).count();
    assert_eq!(unended, 3);
    assert_eq!(tasks[1].kind, TaskKind::Shell);
    assert_eq!(tasks[2].state, TaskState::Stalled);
    assert_eq!(tasks[0].total, Some(48200));
    assert_eq!(tasks[1].total, None);
}

#[test]
fn an_unknown_field_is_refused() {
    let mut value: Value = serde_json::from_str(FRAME).unwrap();
    value["tasks"][0]["extra"] = Value::Bool(true);
    assert!(serde_json::from_value::<PlanMessage>(value).is_err());
}
