use chrono::TimeZone;
use serde_json::{json, Value};

use super::*;
use crate::assets::install::test_env::InstallRootsEnv;

const WRAPPER: &str = "~/.claude/hooks/loom/codex-forward.sh task 'work' --model gpt-5.6-terra --effort xhigh --write";
const UNIT: &str = "--unit-id unit-1";
const INVOCATION: &str = "--invocation-id inv-0123456789abcdef0123456789abcdef";

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 13, 10, 0, 0).unwrap()
}

fn payload(command: &str, response: Value) -> String {
    json!({
        "session_id": "parent-1", "agent_id": "agent-1", "tool_use_id": "tool-1",
        "tool_name": "Bash", "tool_input": {"command": command},
        "tool_response": response,
    })
    .to_string()
}

fn decode(command: &str) -> Result<PayloadForward> {
    decode_payload(&payload(command, json!({"stdout": "out"})), now())
}

fn isolated() -> (tempfile::TempDir, InstallRootsEnv) {
    let temp = tempfile::tempdir().unwrap();
    let env = InstallRootsEnv::isolate(temp.path());
    (temp, env)
}

#[test]
#[serial_test::serial]
fn the_8_10_and_12_word_forms_parse() {
    let _env = isolated();
    for command in [
        WRAPPER.to_owned(),
        format!("{WRAPPER} {UNIT}"),
        format!("{WRAPPER} {UNIT} {INVOCATION}"),
    ] {
        let forward = decode(&command).unwrap();
        assert_eq!(forward.parent_session_id, "parent-1");
        assert_eq!(forward.agent_id, "agent-1");
        assert_eq!(forward.invocation.id, "tool-1");
        assert_eq!(forward.invocation.model, "gpt-5.6-terra");
        assert_eq!(forward.invocation.effort, "xhigh");
        assert_eq!(forward.invocation.timestamp, now());
        let result = forward.invocation.result.unwrap();
        assert_eq!(result.text, "out");
        assert_eq!(result.timestamp, now());
    }
}

#[test]
#[serial_test::serial]
fn commands_the_guard_would_not_authorize_fail() {
    let _env = isolated();
    for command in [
        format!("{WRAPPER} {UNIT} --invocation-id inv-0123456789abcdef0123456789abcde"),
        format!("{WRAPPER} {UNIT} --invocation-id"),
        format!("{WRAPPER} --unit-id ../x"),
        format!("{WRAPPER}; id"),
    ] {
        assert!(decode(&command).is_err(), "{command}");
    }
}

#[test]
#[serial_test::serial]
fn a_non_bash_tool_fails() {
    let _env = isolated();
    let mut value: Value = serde_json::from_str(&payload(WRAPPER, json!({"stdout": ""}))).unwrap();
    value["tool_name"] = json!("Edit");
    assert!(decode_payload(&value.to_string(), now()).is_err());
}

#[test]
#[serial_test::serial]
fn a_missing_tool_result_has_no_result_and_an_unsafe_id_fails_without_echoing_the_payload() {
    let _env = isolated();
    let mut value: Value = serde_json::from_str(&payload(WRAPPER, json!({}))).unwrap();
    value.as_object_mut().unwrap().remove("tool_response");
    let decoded = decode_payload(&value.to_string(), now()).unwrap();
    assert!(decoded.invocation.result.is_none());
    value["tool_response"] = json!("not an object");
    assert!(decode_payload(&value.to_string(), now()).is_err());

    value["tool_response"] = json!({"stdout": ""});
    value["agent_id"] = json!("../secret-agent");
    let error = decode_payload(&value.to_string(), now()).unwrap_err();
    assert!(!error.to_string().contains("secret-agent"));
}

#[test]
#[serial_test::serial]
fn stdout_wins_over_output_and_tool_result_is_the_fallback() {
    let _env = isolated();
    let both = payload(
        WRAPPER,
        json!({"stdout": "from stdout", "output": "from output"}),
    );
    let text = |raw: &str| {
        decode_payload(raw, now())
            .unwrap()
            .invocation
            .result
            .unwrap()
            .text
    };
    assert_eq!(text(&both), "from stdout");
    let output = payload(WRAPPER, json!({"output": "from output"}));
    assert_eq!(text(&output), "from output");

    let mut value: Value = serde_json::from_str(&both).unwrap();
    let response = value
        .as_object_mut()
        .unwrap()
        .remove("tool_response")
        .unwrap();
    value["tool_result"] = response;
    assert_eq!(text(&value.to_string()), "from stdout");
}

#[test]
#[serial_test::serial]
fn background_task_id_is_kept_only_when_safe_and_paths_are_collected() {
    let _env = isolated();
    let raw = payload(
        WRAPPER,
        json!({"stdout": "started", "backgroundTaskId": "task-1", "path": "/x/task-1.output"}),
    );
    let result = decode_payload(&raw, now())
        .unwrap()
        .invocation
        .result
        .unwrap();
    assert_eq!(result.background_task_id.as_deref(), Some("task-1"));
    assert!(result
        .task_output_path_text
        .iter()
        .any(|text| text == "/x/task-1.output"));
    assert_eq!(result.task_output_path_text[0], "started");

    let unsafe_id = payload(WRAPPER, json!({"stdout": "", "background_task_id": "../x"}));
    let result = decode_payload(&unsafe_id, now())
        .unwrap()
        .invocation
        .result
        .unwrap();
    assert_eq!(result.background_task_id, None);
}
