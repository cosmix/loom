//! Bridge tests: the background table over the probe fixtures (plan step A4), written into a
//! harness session up to the moment each check is about.

use serde_json::{json, Value};

use super::background::{TaskKind, TaskState};
use super::fake_host::{settle, wait_until};
use super::fixture_replay::*;
use crate::plan_session::model::SessionState;

/// The hand-back user record of an agent.
pub(super) fn handback(record: &Value) -> bool {
    record["origin"]["handback"] == true
}

/// An agent's last user record, the `SubagentHandback` result.
pub(super) fn ends_turn(record: &Value) -> bool {
    record["toolEndsTurn"] == true
}

/// The `SubagentStop` of agent `a1` (internal forks have other ids).
pub(super) fn a1_stops(record: &Value) -> bool {
    record["event"] == "SubagentStop" && record["payload"]["agent_id"] == "a1"
}

#[test]
fn p1_background_bash_idles_with_an_agent_and_a_shell_until_the_shell_notifies() {
    let h = harness();
    let s = session(&h);
    let mut replay = Replay::load("p1-background-bash", &s);
    // The Stop that lists both tasks.
    replay.until(&s, "2026-11-10T06:37:40.648Z");
    wait_until("both tasks", || tasks(&h, &s).len() == 2);
    h.wait_state(&s, SessionState::Idle);
    let shell = task(&h, &s, "bmiyho9ji").unwrap();
    assert_eq!(
        (shell.kind, shell.state),
        (TaskKind::Shell, TaskState::Running)
    );
    let call = "toolu_01DfQmxWFkRxjV3MGY5sKeAx";
    assert_eq!(shell.tool_use_id.as_deref(), Some(call));
    assert_eq!(
        shell.label.as_deref(),
        Some("Run background sleep and echo")
    );
    assert_eq!(shell.started_at, at("2026-11-10T06:37:36.686Z"));
    let agent = task(&h, &s, "a1").unwrap();
    assert_eq!(agent.kind, TaskKind::Agent);
    assert!(!agent.state.is_ended(), "{agent:?}");
    assert_eq!(agent.label.as_deref(), Some("probe foreground agent"));
    assert_eq!(agent.agent_type.as_deref(), Some("general-purpose"));
    let spawn = "toolu_013aEHXbeZMbuhBHWFDWnMa6";
    assert_eq!(agent.tool_use_id.as_deref(), Some(spawn));
    assert_eq!(
        (agent.steps, agent.last_tool.as_deref()),
        (Some(1), Some("Bash"))
    );

    replay.rest(&s);
    wait_task(&h, &s, "bmiyho9ji", TaskState::Done);
    let agent = state_of(&h, &s, "a1").unwrap();
    assert!(
        !agent.is_ended(),
        "the shell's notification ended the agent"
    );
}

#[test]
fn p1_status_ends_each_task_with_its_outcome_and_stops_the_agent_once() {
    let h = harness();
    let s = session(&h);
    let mut replay = Replay::load("p1-status", &s);
    replay.until(&s, "2026-11-10T09:38:02.114Z");
    wait_task(&h, &s, "badtfi3uv", TaskState::Failed);
    replay.until(&s, "2026-11-10T09:38:11.580Z");
    wait_task(&h, &s, "bx0h460uz", TaskState::Stopped);
    settle();
    let before = (published(&h, &s), tasks(&h, &s));
    // The next Stop, whose list no longer names the stopped shell.
    replay.until(&s, "2026-11-10T09:38:12.832Z");
    settle();
    assert_eq!(
        (published(&h, &s), tasks(&h, &s)),
        before,
        "the list moved the table"
    );

    // The TaskStop records follow an enqueue and a 49.427 api-request in the file, so they
    // are read at that time.
    replay.until(&s, "2026-11-10T09:38:49.427Z");
    wait_task(&h, &s, "a1", TaskState::Stopped);
    assert_eq!(stopped(&h, "a1"), 1);
    // The stopped agent's last records, the empty list and its `killed` notification.
    replay.rest(&s);
    settle();
    let agent = task(&h, &s, "a1").unwrap();
    assert_eq!(agent.state, TaskState::Stopped, "{agent:?}");
    assert_eq!(agent.last_activity_at, at("2026-11-10T09:38:51.985Z"));
    assert_eq!(
        stopped(&h, "a1"),
        1,
        "the killed notification stopped it again"
    );
}

#[test]
fn each_agent_end_evidence_fed_alone_stops_the_agent_once() {
    let evidence: [fn(&Value) -> bool; 3] = [handback, ends_turn, a1_stops];
    for kept in 0..evidence.len() {
        let h = harness();
        let s = session(&h);
        let dropped = |record: &Value| {
            let mut others = evidence.iter().enumerate();
            others.any(|(at, drop)| at != kept && drop(record))
        };
        let replay = Replay::load("p1-background-agent", &s).unfinished();
        let mut replay = replay.without(dropped);
        // Up to the hand-back, which follows a 58.759 enqueue in the file, and before the Stop
        // whose empty list would end the agent too.
        replay.until(&s, "2026-11-10T07:06:58.759Z");
        wait_task(&h, &s, "a1", TaskState::Done);
        assert_eq!(stopped(&h, "a1"), 1, "evidence {kept}");
        let queued = fixture_record("p1-background-agent", &s, |record| {
            let content = record["content"].as_str().unwrap_or_default();
            record["type"] == "queue-operation" && content.starts_with("<task-notification>")
        });
        h.record(&s, notification(&queued["content"]));
        settle();
        assert_eq!(
            stopped(&h, "a1"),
            1,
            "evidence {kept}: the notification stopped it again"
        );
        assert_eq!(state_of(&h, &s, "a1"), Some(TaskState::Done));
    }
}

/// The user record Claude Code writes for a queued notification, as `p1-status.jsonl`'s.
fn notification(body: &Value) -> Value {
    json!({"type": "user", "message": {"role": "user", "content": body},
        "origin": {"kind": "task-notification", "producer": "session-task"},
        "promptSource": "system", "turnOrigin": "task_notification"})
}

#[test]
fn p1_interrupt_keeps_the_tasks_and_stop_lists_adopt_and_end_them() {
    let h = harness();
    let s = session(&h);
    // Without the agent's own end evidence, the Stop list is what ends it.
    let replay = Replay::load("p1-interrupt", &s).unfinished();
    let mut replay =
        replay.without(|record| handback(record) || ends_turn(record) || a1_stops(record));
    replay.until(&s, "2026-11-10T07:20:25.055Z");
    wait_until("the launches", || tasks(&h, &s).len() == 2);
    settle();
    for id in ["a1", "bsngt0yiw"] {
        let state = state_of(&h, &s, id).unwrap();
        assert!(!state.is_ended(), "the interrupt ended {id}: {state:?}");
    }
    // The Stop that names both shells and no longer the agent.
    replay.until(&s, "2026-11-10T07:20:32.465Z");
    wait_task(&h, &s, "a1", TaskState::Done);
    assert_eq!(stopped(&h, "a1"), 1);
    let adopted = task(&h, &s, "bhupkzrs5").expect("the subagent's shell");
    assert_eq!(
        (adopted.kind, adopted.state),
        (TaskKind::Shell, TaskState::Running)
    );
    assert_eq!(adopted.tool_use_id, None);
    assert_eq!(adopted.label.as_deref(), Some("Run sleep in background"));
    assert_eq!(adopted.started_at, at("2026-11-10T07:20:32.465Z"));
    // The queued interim notification, then two lists naming both shells.
    replay.rest(&s);
    settle();
    assert_eq!(stopped(&h, "a1"), 1);
    for shell in ["bsngt0yiw", "bhupkzrs5"] {
        assert_eq!(state_of(&h, &s, shell), Some(TaskState::Running), "{shell}");
    }
    h.spool(&s, "Stop", json!({"background_tasks": []}));
    wait_until("both shells ended", || {
        ["bsngt0yiw", "bhupkzrs5"]
            .iter()
            .all(|shell| state_of(&h, &s, shell) == Some(TaskState::Done))
    });
}

#[test]
fn a_failed_notification_turns_a_shell_a_list_ended_into_failed() {
    let h = harness();
    let s = session(&h);
    let mut replay = Replay::load("p1-status", &s);
    // Spool first: the lists adopt the shell and end it before its launch is read.
    replay.spool(&s);
    wait_task(&h, &s, "badtfi3uv", TaskState::Done);
    assert_eq!(task(&h, &s, "badtfi3uv").unwrap().tool_use_id, None);
    replay.rest(&s);
    wait_task(&h, &s, "badtfi3uv", TaskState::Failed);
    let shell = task(&h, &s, "badtfi3uv").unwrap();
    let call = "toolu_01RsWxunPyoVUUXBkZUVm5QN";
    assert_eq!(shell.tool_use_id.as_deref(), Some(call));
    let label = "Run background sleep then exit with code 7";
    assert_eq!(shell.label.as_deref(), Some(label));
    assert_eq!(shell.started_at, at("2026-11-10T09:37:59.041Z"));
    assert_eq!(stopped(&h, "badtfi3uv"), 0, "a shell is no subagent");
}

#[test]
fn compact_keeps_the_table_and_a_startup_clears_it() {
    let h = harness();
    let s = session(&h);
    let mut replay = Replay::load("p1-compact", &s);
    replay.until(&s, "2026-11-10T06:37:57.631Z");
    wait_task(&h, &s, "bymo0084r", TaskState::Running);
    settle();
    let before = (published(&h, &s), tasks(&h, &s));
    // PreCompact, the two forks' SubagentStops (`agent_type: ""`) and SessionStart compact.
    replay.until(&s, "2026-11-10T06:38:05.124Z");
    settle();
    assert_eq!(
        (published(&h, &s), tasks(&h, &s)),
        before,
        "the compact moved the table"
    );
    h.spool(&s, "SessionStart", json!({"source": "startup"}));
    wait_until("the table cleared", || {
        published(&h, &s) > before.0 && tasks(&h, &s).is_empty()
    });
}

#[test]
fn a_fork_skill_is_one_agent_without_a_tool_use_id_ended_by_its_notification() {
    let h = harness();
    let s = session(&h);
    let replay = Replay::load("p2-skills", &s).unfinished();
    let mut replay = replay.without(a1_stops);
    replay.until(&s, "2026-11-10T06:40:16.271Z");
    wait_until("the fork", || task(&h, &s, "a1").is_some());
    settle();
    let fork = task(&h, &s, "a1").unwrap();
    assert_eq!(fork.kind, TaskKind::Agent);
    assert!(!fork.state.is_ended(), "{fork:?}");
    assert_eq!(fork.tool_use_id, None);
    assert_eq!(fork.label.as_deref(), Some("/probe-fork"));
    assert_eq!(fork.agent_type.as_deref(), Some("general-purpose"));
    replay.rest(&s);
    wait_task(&h, &s, "a1", TaskState::Done);
    assert_eq!(tasks(&h, &s).len(), 1, "{:?}", tasks(&h, &s));
    assert_eq!(task(&h, &s, "a1").unwrap().tool_use_id, None);
    assert_eq!(stopped(&h, "a1"), 1);
}
