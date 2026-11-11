//! Contracts of the plan server's background frame: every string from a transcript, a meta file
//! or a `Stop` list is flattened on the wire, and `install_with` starts the planner token pass
//! whose totals the frame joins by agent id.

use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};

use super::frame;
use crate::plan_server::chat::tests::{browser, Fixture};
use crate::plan_session::bridge::background::{BackgroundTask, TaskKind, TaskState};
use crate::tokens::{AgentFlow, AgentKind, TokenUsage};

/// Every string value inside `value`.
fn strings<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        _ => {}
    }
}

fn hostile(text: &str) -> bool {
    text.chars()
        .any(|ch| ch.is_control() || ch == '\u{202e}' || ch == '\u{200b}')
}

fn agent_task() -> BackgroundTask {
    let at = Utc::now();
    BackgroundTask {
        id: "a1".to_owned(),
        kind: TaskKind::Agent,
        label: Some("evil\u{1b}[31m\u{202e}x\ny".to_owned()),
        agent_type: Some("Ex\u{7}plore".to_owned()),
        tool_use_id: Some("toolu_f1".to_owned()),
        state: TaskState::Running,
        started_at: at,
        last_activity_at: at,
        steps: Some(3),
        last_tool: Some("Re\u{200b}ad".to_owned()),
        report_preview: Some("ok\u{1b}]0;t\u{7}".to_owned()),
    }
}

#[test]
fn the_background_frame_flattens_transcript_text() {
    let usage = TokenUsage {
        input: 10,
        cache_write: 0,
        cache_read: 5,
        output: 20,
    };
    let flow = AgentFlow {
        id: "a1".to_owned(),
        kind: AgentKind::Subagent,
        agent_type: Some("Explore".to_owned()),
        forwarder: None,
        model: Some("claude\u{1b}[2J".to_owned()),
        active: true,
        total: usage,
        rate: usage,
        series: vec![1],
    };
    let value = serde_json::to_value(frame("s1", &[agent_task()], &[flow])).unwrap();
    let mut found = Vec::new();
    strings(&value, &mut found);
    for text in found {
        assert!(!hostile(text), "{text:?} reached the wire raw: {value}");
    }
    let row = &value["tasks"][0];
    let label = row["label"].as_str().unwrap_or_default();
    assert!(label.contains("evil"), "the label was dropped: {row}");
    let model = row["model"].as_str().unwrap_or_default();
    assert!(model.starts_with("claude"), "the model was dropped: {row}");
}

/// One subagent assistant record with usage, shaped as `subagent-agent.jsonl`'s, stamped now.
fn usage_record(session: &str) -> Value {
    let usage = json!({"input_tokens": 1200, "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": 0, "output_tokens": 300});
    json!({"type": "assistant", "uuid": "w1", "parentUuid": null, "isSidechain": true,
        "agentId": "a1", "sessionId": session, "requestId": "req_w1",
        "timestamp": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        "message": {"id": "msg_w1", "model": "claude-haiku-5", "role": "assistant",
            "content": [{"type": "text", "text": "Reading the tracker."}],
            "stop_reason": null, "usage": usage}})
}

/// A background frame for `session` whose `a1` task carries a total above 0.
fn counted(frames: &[Value], session: &str) -> bool {
    let ours = frames
        .iter()
        .filter(|frame| frame["type"] == "background" && frame["session"] == session);
    let rows = ours.flat_map(|frame| frame["tasks"].as_array().cloned().unwrap_or_default());
    rows.into_iter()
        .any(|row| row["id"] == "a1" && row["total"].as_u64().is_some_and(|total| total > 0))
}

#[test]
fn install_with_publishes_background_token_totals() {
    let f = Fixture::new(false);
    let tab = f.connect(&browser());
    let s = f.idle_chat(&tab);
    let transcripts = f.bridge.transcripts(&s.id);
    let main = transcripts
        .and_then(|t| t.main)
        .expect("the chat's main transcript");
    let dir = main.with_extension("").join("subagents");
    fs::create_dir_all(&dir).unwrap();
    let meta = json!({"agentType": "Explore", "toolUseId": "toolu_w1"});
    fs::write(dir.join("agent-a1.meta.json"), meta.to_string()).unwrap();
    let line = format!("{}\n", usage_record(&s.id));
    fs::write(dir.join("agent-a1.jsonl"), line).unwrap();
    f.spool(
        &s,
        "SubagentStart",
        json!({"agent_id": "a1", "agent_type": "Explore"}),
    );

    let deadline = Instant::now() + Duration::from_secs(20);
    while !counted(&tab.frames(), &s.id) {
        let frames = tab.frames();
        let seen: Vec<&Value> = frames
            .iter()
            .filter(|f| f["type"] == "background")
            .collect();
        assert!(Instant::now() < deadline, "no a1 total in {seen:?}");
        thread::sleep(Duration::from_millis(50));
    }
}
