//! Bridge tests: the background table's turns and clears in a harness session, and a replay
//! from offset 0 against the live run.

use std::fs;

use serde_json::{json, Value};

use super::background::{BackgroundTask, TaskState};
use super::fake_host::{
    append_record, assistant_tools, settle, tool_result, wait_until, Harness, Sess,
};
use super::fixture_replay::*;
use super::tests_queue::delivery;
use super::BridgeEvent;
use crate::plan_session::model::{Delivery, SessionState};

/// An Agent call and its `async_launched` result, as `probe-notes.md` "Agent launch result".
fn launch_agent(h: &Harness, s: &Sess, tool_use: &str, agent: &str) {
    let input = json!({"description": "probe background agent", "prompt": "Reply DONE.",
        "run_in_background": "true", "subagent_type": "general-purpose"});
    h.record(
        s,
        assistant_tools(&format!("msg_{tool_use}"), &[(tool_use, "Agent", input)]),
    );
    let result = json!({"isAsync": true, "status": "async_launched", "agentId": agent,
        "description": "probe background agent", "canContinueAgent": false});
    h.record(s, tool_result(tool_use, result));
}

/// A background Bash call and its result, as `probe-notes.md` "Bash launch result".
fn launch_shell(h: &Harness, s: &Sess, tool_use: &str, task: &str) {
    let input = json!({"command": "sleep 5; echo DONE", "run_in_background": true,
        "description": "Run background sleep and echo"});
    h.record(
        s,
        assistant_tools(&format!("msg_{tool_use}"), &[(tool_use, "Bash", input)]),
    );
    let result = json!({"backgroundTaskId": task, "interrupted": false, "stdout": ""});
    h.record(s, tool_result(tool_use, result));
}

/// One assistant line of agent `agent`'s transcript, stamped now.
fn agent_works(s: &Sess, agent: &str, message: &str) {
    let dir = subagents(s);
    let meta = dir.join(format!("agent-{agent}.meta.json"));
    if !meta.exists() {
        fs::write(meta, json!({"agentType": "general-purpose"}).to_string()).unwrap();
    }
    let read = json!({"type": "tool_use", "id": format!("toolu_{message}"), "name": "Read",
        "input": {"file_path": "/tmp/notes.md"}});
    let line = json!({"type": "assistant", "message": {"id": message, "role": "assistant",
        "content": [read], "stop_reason": "tool_use"}});
    append_record(&dir.join(format!("agent-{agent}.jsonl")), line, &s.id);
}

#[test]
fn an_interim_notification_ends_the_agent_and_its_next_record_reopens_it() {
    let h = harness();
    let s = session(&h);
    h.working(&s);
    launch_agent(&h, &s, "toolu_01KoWA6n7ZZEw6Z3s5sNV1NJ", "a1");
    h.spool(
        &s,
        "SubagentStart",
        json!({"agent_id": "a1", "agent_type": "general-purpose"}),
    );
    agent_works(&s, "a1", "msg_w1");
    wait_task(&h, &s, "a1", TaskState::ToolWait);
    let interim = fixture_record("p1-interrupt", &s, |record| {
        record["origin"]["kind"] == "task-notification"
    });
    let body = interim["message"]["content"].as_str().unwrap();
    assert!(
        body.contains("is waiting on its own background work"),
        "{body}"
    );
    h.record(&s, interim);
    wait_task(&h, &s, "a1", TaskState::Done);
    assert_eq!(stopped(&h, "a1"), 1);
    agent_works(&s, "a1", "msg_w2");
    wait_task(&h, &s, "a1", TaskState::ToolWait);
    settle();
    assert_eq!(stopped(&h, "a1"), 1, "the reopened agent was stopped again");
    assert_eq!(task(&h, &s, "a1").unwrap().steps, Some(2));
}

#[test]
fn a_notification_ends_its_task_and_opens_a_turn_without_confirming_the_pasted_message() {
    let h = harness();
    let s = session(&h);
    h.working(&s);
    launch_shell(&h, &s, "toolu_01DfQmxWFkRxjV3MGY5sKeAx", "bmiyho9ji");
    h.spool(&s, "Stop", json!({}));
    h.wait_state(&s, SessionState::Idle);
    h.bridge.send(&s.id, "A".to_owned(), Vec::new()).unwrap();
    wait_until("A pasted", || h.sent(&s) == ["A"]);
    let begun = h.pins.begins.lock().unwrap().len();
    let notified = fixture_record("p1-background-bash", &s, |record| {
        record["origin"]["kind"] == "task-notification"
    });
    let prompt = notified["message"]["content"].clone();
    h.record(&s, notified);
    wait_task(&h, &s, "bmiyho9ji", TaskState::Done);
    wait_until("the notification's turn", || {
        h.pins.begins.lock().unwrap().len() > begun
    });
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": prompt }));
    let handback = "<agent-message from=\"a1\">\n[Subagent hand-back] The report follows:\n  DONE\n</agent-message>";
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": handback }));
    settle();
    assert_eq!(
        delivery(&h, "A"),
        Some(Delivery::Sent),
        "a background prompt confirmed A"
    );
}

#[test]
fn agents_killed_clears_every_task() {
    let h = harness();
    let s = session(&h);
    h.working(&s);
    for agent in ["a1", "a2"] {
        h.spool(
            &s,
            "SubagentStart",
            json!({"agent_id": agent, "agent_type": "Explore"}),
        );
    }
    launch_shell(&h, &s, "toolu_b1", "bshell01");
    wait_until("three tasks", || tasks(&h, &s).len() == 3);
    let killed = json!({"type": "system", "subtype": "agents_killed",
        "content": "Background agents stopped", "level": "info"});
    h.record(&s, killed);
    wait_until("the table cleared", || tasks(&h, &s).is_empty());
    assert_eq!(stopped(&h, "a1") + stopped(&h, "a2"), 0);
}

/// The table and the `SubagentStopped` count a fixture leaves, written by `write`.
fn final_table(prefix: &str, write: fn(&mut Replay, &Sess)) -> (Vec<BackgroundTask>, usize) {
    let h = harness();
    let s = session(&h);
    let mut replay = Replay::load(prefix, &s);
    write(&mut replay, &s);
    settle();
    settle();
    let ours = |event: &BridgeEvent| matches!(event, BridgeEvent::SubagentStopped { id, .. } if *id == s.id);
    (tasks(&h, &s), h.count_events(ours))
}

#[test]
fn a_replay_from_offset_0_rebuilds_the_live_table_in_either_read_order() {
    for prefix in ["p1-interrupt", "p1-status"] {
        let live = final_table(prefix, Replay::live);
        assert!(live.0.len() >= 3 && live.1 == 1, "{prefix}: {live:?}");
        assert_eq!(final_table(prefix, Replay::rest), live, "{prefix}: replay");
        let spool_first = final_table(prefix, Replay::spool_first);
        assert_eq!(spool_first, live, "{prefix}: spool first");
    }
}

#[test]
fn a_task_value_is_its_wire_form() {
    let h = harness();
    let s = session(&h);
    h.working(&s);
    launch_shell(&h, &s, "toolu_b1", "bshell01");
    wait_until("the shell", || task(&h, &s, "bshell01").is_some());
    let shell: Value = serde_json::to_value(task(&h, &s, "bshell01").unwrap()).unwrap();
    assert_eq!(shell["kind"], "shell");
    assert_eq!(shell["state"], "running");
    assert_eq!(shell["steps"], Value::Null);
    let started = shell["started_at"].as_str().unwrap();
    assert!(
        chrono::DateTime::parse_from_rfc3339(started).is_ok(),
        "{started}"
    );
}
