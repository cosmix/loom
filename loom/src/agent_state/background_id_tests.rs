use serde_json::{json, Value};

use super::background_task_id;

fn status_fixture_results() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/plan_session/transcripts/p1-status.jsonl"
    );
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row.get("type").and_then(Value::as_str) == Some("user"))
        .filter_map(|row| row.get("toolUseResult").filter(|v| v.is_object()).cloned())
        .collect()
}

#[test]
fn the_two_bash_launches_carry_task_ids_and_the_agent_launch_none() {
    let ids: Vec<String> = status_fixture_results()
        .iter()
        .filter_map(background_task_id)
        .collect();
    assert_eq!(ids, ["badtfi3uv", "bx0h460uz"]);
}

#[test]
fn snake_case_key_is_read_when_the_camel_case_key_is_absent() {
    let result = json!({"background_task_id": "task-1"});
    assert_eq!(background_task_id(&result).as_deref(), Some("task-1"));
}

#[test]
fn an_unsafe_id_yields_none() {
    assert_eq!(
        background_task_id(&json!({"backgroundTaskId": "../x"})),
        None
    );
    assert_eq!(background_task_id(&json!({"backgroundTaskId": 7})), None);
    assert_eq!(background_task_id(&json!({})), None);
}
