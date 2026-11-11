//! Contracts of the background table: an agent ends once across all its end evidence, the close
//! gate holds a summary session for a launched agent, a `Stop` list adopts and ends a shell, and
//! a quiet agent turns stalled on a tick with no new bytes.

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::background::{BackgroundTask, TaskKind, TaskState};
use super::fake_host::{
    append_record, assistant_tools, settle, tool_result, wait_until, Harness, Sess, OPENING,
};
use super::{BridgeEvent, EndOutcome};
use crate::plan_session::kind::SessionKind::{self, Authoring};
use crate::plan_session::model::SessionState;

/// The tasks of the last `Background` event for `s`.
fn tasks(h: &Harness, s: &Sess) -> Vec<BackgroundTask> {
    let events = h.events.lock().unwrap();
    let last = events.iter().rev().find_map(|event| match event {
        BridgeEvent::Background { id, tasks } if *id == s.id => Some(tasks.clone()),
        _ => None,
    });
    last.unwrap_or_default()
}

fn task(h: &Harness, s: &Sess, id: &str) -> Option<BackgroundTask> {
    tasks(h, s).into_iter().find(|task| task.id == id)
}

fn state_of(h: &Harness, s: &Sess, id: &str) -> Option<TaskState> {
    task(h, s, id).map(|task| task.state)
}

fn stopped(h: &Harness, agent: &str) -> usize {
    h.count_events(
        |e| matches!(e, BridgeEvent::SubagentStopped { agent_id, .. } if agent_id == agent),
    )
}

/// The Agent call and its `async_launched` result, as `probe-notes.md` "Agent launch result".
fn launch(h: &Harness, s: &Sess, tool_use: &str, agent: &str) {
    let input = json!({"description": "probe background agent", "prompt": "Reply DONE.",
        "run_in_background": "true", "subagent_type": "general-purpose"});
    let call = assistant_tools(&format!("msg_{tool_use}"), &[(tool_use, "Agent", input)]);
    h.record(s, call);
    let result = json!({"isAsync": true, "status": "async_launched", "agentId": agent,
        "description": "probe background agent", "resolvedModel": "claude-sonnet-5-5",
        "outputFile": format!("/tmp/tasks/{agent}.output"), "canReadOutputFile": true,
        "canContinueAgent": false});
    h.record(s, tool_result(tool_use, result));
}

/// The hand-back user record, as `probe-notes.md` "Agent hand-back".
fn handback(agent: &str) -> Value {
    let body = "[Subagent hand-back] The report follows:\n  DONE";
    let content = format!(
        "Another Claude session sent a message:\n<agent-message from=\"{agent}\">\n{body}\n</agent-message>"
    );
    json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": content},
        "origin": {"kind": "peer", "from": agent, "senderTaskId": agent,
            "name": "general-purpose", "body": body, "handback": true,
            "handbackReportText": "DONE"},
        "promptSource": "system", "turnOrigin": "peer"})
}

/// A task-notification user record, as `probe-notes.md` "Notification carriers and body".
fn notification(task: &str, tool_use: &str, status: &str, summary: &str) -> Value {
    let content = format!(
        "<task-notification>\n<task-id>{task}</task-id>\n<tool-use-id>{tool_use}</tool-use-id>\n\
         <output-file>/tmp/tasks/{task}.output</output-file>\n<status>{status}</status>\n\
         <summary>{summary}</summary>\n</task-notification>"
    );
    json!({"type": "user", "message": {"role": "user", "content": content},
        "origin": {"kind": "task-notification", "producer": "session-task", "runId": "r1"},
        "promptSource": "system", "turnOrigin": "task_notification"})
}

fn finished(task: &str, tool_use: &str) -> Value {
    let summary = "Agent \"probe background agent\" finished";
    notification(task, tool_use, "completed", summary)
}

fn running_agent(agent: &str) -> Value {
    json!({"agent_type": "general-purpose", "description": "probe background agent",
        "id": agent, "status": "running", "type": "subagent"})
}

fn subagents(s: &Sess) -> PathBuf {
    let dir = s.transcript.with_extension("").join("subagents");
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn an_agent_ends_once_across_every_end_evidence() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    launch(&h, &s, "toolu_c2", "a1");
    wait_until("the launched agent", || task(&h, &s, "a1").is_some());
    let start = json!({"agent_id": "a1", "agent_type": "general-purpose"});
    h.spool(&s, "SubagentStart", start);
    settle();
    h.record(&s, handback("a1"));
    wait_until("the hand-back end", || {
        state_of(&h, &s, "a1") == Some(TaskState::Done)
    });
    let stop = json!({"agent_id": "a1", "agent_type": "general-purpose", "background_tasks": []});
    h.spool(&s, "SubagentStop", stop);
    settle();
    h.record(&s, finished("a1", "toolu_c2"));
    settle();
    assert_eq!(stopped(&h, "a1"), 1, "SubagentStopped per end evidence");
    let a1 = task(&h, &s, "a1").expect("a1 in the last Background event");
    assert_eq!(a1.kind, TaskKind::Agent);
    assert_eq!(a1.state, TaskState::Done);
}

#[test]
fn a_summary_session_waits_for_a_launched_agent() {
    let h = Harness::ready(|_| {});
    let s = h.start(SessionKind::Summary);
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the opening", || h.host.delivered(&s.socket) == [OPENING]);
    h.spool(&s, "UserPromptSubmit", json!({"prompt": OPENING}));
    launch(&h, &s, "toolu_s1", "a1");
    wait_until("the launched agent", || task(&h, &s, "a1").is_some());
    h.spool(
        &s,
        "Stop",
        json!({"background_tasks": [running_agent("a1")]}),
    );
    settle();
    assert_eq!(h.ended(&s.id), None, "closed while its agent ran");

    h.record(&s, handback("a1"));
    wait_until("the hand-back end", || {
        state_of(&h, &s, "a1") == Some(TaskState::Done)
    });
    let prompt = "<agent-message from=\"a1\">\n[Subagent hand-back] DONE\n</agent-message>";
    h.spool(&s, "UserPromptSubmit", json!({ "prompt": prompt }));
    h.spool(&s, "Stop", json!({"background_tasks": []}));
    wait_until("the end", || h.ended(&s.id) == Some(EndOutcome::Closed));
}

#[test]
fn a_stop_list_adopts_and_ends_a_subagent_shell() {
    let h = Harness::ready(|_| {});
    let s = h.idle(Authoring);
    h.working(&s);
    let shell = json!({"command": "sleep 45", "description": "Run sleep in background",
        "id": "bhupkzrs5", "status": "running", "type": "shell"});
    h.spool(&s, "Stop", json!({ "background_tasks": [shell] }));
    wait_until("the adopted shell", || task(&h, &s, "bhupkzrs5").is_some());
    let adopted = task(&h, &s, "bhupkzrs5").unwrap();
    assert_eq!(adopted.kind, TaskKind::Shell);
    assert_eq!(adopted.tool_use_id, None);
    assert_eq!(adopted.label.as_deref(), Some("Run sleep in background"));
    assert_eq!(adopted.state, TaskState::Running);

    h.wait_state(&s, SessionState::Idle);
    h.working(&s);
    h.spool(&s, "Stop", json!({"background_tasks": []}));
    wait_until("the omitted shell ended", || {
        state_of(&h, &s, "bhupkzrs5") == Some(TaskState::Done)
    });
}

#[test]
fn a_quiet_agent_turns_stalled_on_a_tick() {
    let h = Harness::ready(|_| {});
    h.bridge.lock().stall_budget = Duration::from_secs(1);
    let s = h.idle(Authoring);
    h.working(&s);
    launch(&h, &s, "toolu_q1", "a1");
    let start = json!({"agent_id": "a1", "agent_type": "general-purpose"});
    h.spool(&s, "SubagentStart", start);
    let dir = subagents(&s);
    let meta = json!({"agentType": "general-purpose", "toolUseId": "toolu_q1"});
    fs::write(dir.join("agent-a1.meta.json"), meta.to_string()).unwrap();
    let read = json!({"type": "tool_use", "id": "toolu_r1", "name": "Read",
        "input": {"file_path": "/tmp/notes.md"}});
    let line = json!({"type": "assistant", "message": {"id": "msg_q1", "role": "assistant",
        "content": [read], "stop_reason": "tool_use"}});
    append_record(&dir.join("agent-a1.jsonl"), line, &s.id);

    let deadline = Instant::now() + Duration::from_secs(6);
    while state_of(&h, &s, "a1") != Some(TaskState::Stalled) {
        let seen = task(&h, &s, "a1");
        assert!(
            Instant::now() < deadline,
            "a1 never turned stalled: {seen:?}"
        );
        thread::sleep(Duration::from_millis(20));
    }
    h.record(&s, finished("a1", "toolu_q1"));
    wait_until("the notification end", || {
        state_of(&h, &s, "a1") == Some(TaskState::Done)
    });
}
