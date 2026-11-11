//! The table's rules fed directly: ranks and idempotence, `Stop` lists in either read order,
//! outcomes over list ends, reopening, the quiet tick, teammates and clears.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::*;
use crate::plan_session::transcript::notification::{parse_task_notification, BackgroundEvidence};

fn at(secs: i64) -> DateTime<Utc> {
    "2026-11-10T07:00:00Z".parse::<DateTime<Utc>>().unwrap() + chrono::Duration::seconds(secs)
}

fn spool(event: &str, secs: i64, payload: Value) -> SpoolEvent {
    SpoolEvent {
        received_at: at(secs),
        event: event.to_owned(),
        session_id: None,
        transcript_path: None,
        request_id: None,
        payload,
    }
}

fn evidence(table: &mut BackgroundTasks, secs: i64, evidence: BackgroundEvidence) -> Change {
    table.mark(&RecordMark::Background {
        at: at(secs),
        evidence,
    })
}

fn shell(task: &str, tool_use: &str, label: &str) -> BackgroundEvidence {
    BackgroundEvidence::ShellLaunch {
        task_id: task.to_owned(),
        tool_use_id: Some(tool_use.to_owned()),
        label: Some(label.to_owned()),
    }
}

fn agent(id: &str, tool_use: &str) -> BackgroundEvidence {
    BackgroundEvidence::AgentLaunch {
        agent_id: id.to_owned(),
        tool_use_id: Some(tool_use.to_owned()),
        label: Some("Survey the bridge".to_owned()),
    }
}

fn notified(task: &str, status: &str, summary: &str) -> BackgroundEvidence {
    let body = format!(
        "<task-notification>\n<task-id>{task}</task-id>\n<status>{status}</status>\n\
         <summary>{summary}</summary>\n</task-notification>"
    );
    BackgroundEvidence::Notification(parse_task_notification(&body).unwrap())
}

fn stop(secs: i64, listed: &[(&str, &str)]) -> SpoolEvent {
    let list: Vec<Value> = listed
        .iter()
        .map(|(id, kind)| {
            json!({"id": id, "type": kind, "status": "running",
            "description": format!("listed {id}")})
        })
        .collect();
    spool("Stop", secs, json!({ "background_tasks": list }))
}

fn get(table: &BackgroundTasks, id: &str) -> BackgroundTask {
    let tasks = table.tasks();
    tasks.into_iter().find(|task| task.id == id).expect(id)
}

/// An assistant line of an agent transcript at `secs`, calling `tool`.
fn line(secs: i64, tool: &str) -> Value {
    let stamp = at(secs).to_rfc3339();
    json!({"type": "assistant", "timestamp": stamp, "message": {"role": "assistant",
        "content": [{"type": "tool_use", "id": "toolu_x", "name": tool, "input": {}}]}})
}

#[test]
fn launch_evidence_twice_changes_nothing_and_the_higher_rank_wins_in_any_order() {
    let mut table = BackgroundTasks::default();
    let start = spool(
        "SubagentStart",
        5,
        json!({"agent_id": "a1", "agent_type": "Explore"}),
    );
    assert!(table.event(&start).changed);
    assert!(evidence(&mut table, 3, agent("a1", "toolu_a")).changed);
    assert_eq!(
        evidence(&mut table, 3, agent("a1", "toolu_a")),
        Change::default()
    );
    let meta = AgentMeta {
        description: Some("from the meta".to_owned()),
        agent_type: Some("general-purpose".to_owned()),
        ..AgentMeta::default()
    };
    assert_eq!(table.meta("a1", &meta), Change::default());
    let a1 = get(&table, "a1");
    assert_eq!(a1.label.as_deref(), Some("Survey the bridge"));
    assert_eq!(a1.agent_type.as_deref(), Some("Explore"));
    assert_eq!((a1.started_at, a1.last_activity_at), (at(3), at(5)));
    assert_eq!(table.blocking(), 1);
}

#[test]
fn a_stop_list_adopts_shells_never_agents_and_ends_what_it_omits() {
    let mut table = BackgroundTasks::default();
    evidence(&mut table, 1, shell("b1", "toolu_b", "Run it"));
    table.event(&stop(
        2,
        &[("b2", "shell"), ("ghost", "subagent"), ("b1", "shell")],
    ));
    assert_eq!(get(&table, "b2").label.as_deref(), Some("listed b2"));
    assert_eq!(get(&table, "b2").tool_use_id, None);
    assert!(table.tasks().iter().all(|task| task.id != "ghost"));
    // The launch outranks the list's label; the list is no sighting.
    assert_eq!(get(&table, "b1").label.as_deref(), Some("Run it"));
    assert_eq!(get(&table, "b1").last_activity_at, at(1));
    table.event(&stop(4, &[("b2", "shell")]));
    assert_eq!(get(&table, "b1").state, TaskState::Done);
    assert_eq!(get(&table, "b2").state, TaskState::Running);
    // A SubagentStop's list lags and is never read.
    table.event(&spool(
        "SubagentStop",
        5,
        json!({"agent_id": "x", "background_tasks": []}),
    ));
    assert_eq!(get(&table, "b2").state, TaskState::Running);
    // Read after a newer list that omits it, an older launch ends at once.
    let late = evidence(&mut table, 3, shell("b3", "toolu_c", "Late"));
    assert_eq!(late.stopped, Vec::<String>::new());
    assert_eq!(get(&table, "b3").state, TaskState::Done);
}

#[test]
fn an_outcome_replaces_a_list_end_and_stops_an_agent_once() {
    let mut table = BackgroundTasks::default();
    table.event(&spool("SubagentStart", 1, json!({"agent_id": "a1"})));
    let ended = table.event(&stop(2, &[]));
    assert_eq!(ended.stopped, ["a1"]);
    let killed = evidence(
        &mut table,
        3,
        notified("a1", "killed", "Agent \"x\" was stopped"),
    );
    assert!(killed.changed && killed.stopped.is_empty(), "{killed:?}");
    assert_eq!(get(&table, "a1").state, TaskState::Stopped);
    // A notification for an unknown id adds an ended entry of the kind its summary names.
    evidence(
        &mut table,
        4,
        notified("b9", "failed", "Background command \"y\" failed"),
    );
    assert_eq!(
        (get(&table, "b9").kind, get(&table, "b9").state),
        (TaskKind::Shell, TaskState::Failed)
    );
    // A cleared table forgets the entries, never the stops it reported.
    table.event(&spool("SessionStart", 5, json!({"source": "clear"})));
    assert!(table.tasks().is_empty());
    let again = evidence(
        &mut table,
        6,
        notified("a1", "completed", "Agent \"x\" finished"),
    );
    assert_eq!(again.stopped, Vec::<String>::new());
}

#[test]
fn a_task_stop_ends_a_known_task_and_a_later_list_changes_nothing() {
    let mut table = BackgroundTasks::default();
    evidence(&mut table, 1, shell("b1", "toolu_b", "Run it"));
    let stopped = BackgroundEvidence::TaskStopped {
        task_id: "b1".to_owned(),
    };
    assert!(evidence(&mut table, 2, stopped.clone()).changed);
    assert_eq!(get(&table, "b1").state, TaskState::Stopped);
    assert_eq!(table.event(&stop(3, &[])), Change::default());
    let unknown = BackgroundEvidence::TaskStopped {
        task_id: "b2".to_owned(),
    };
    assert_eq!(evidence(&mut table, 4, unknown), Change::default());
}

#[test]
fn only_a_later_assistant_record_or_start_reopens_an_ended_agent() {
    let mut table = BackgroundTasks::default();
    evidence(&mut table, 1, agent("a1", "toolu_a"));
    table.agent_line("a1", &line(2, "Read"));
    assert_eq!(get(&table, "a1").state, TaskState::ToolWait);
    let killed = BackgroundEvidence::TaskStopped {
        task_id: "a1".to_owned(),
    };
    assert_eq!(evidence(&mut table, 3, killed).stopped, ["a1"]);
    // A stopped agent's own rejected result and interrupt record come after its TaskStop.
    let rejected = json!({"type": "user", "timestamp": at(4).to_rfc3339(),
        "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t"}]}});
    table.agent_line("a1", &rejected);
    assert_eq!(get(&table, "a1").state, TaskState::Stopped);
    // An older record read late never reopens it.
    table.agent_line("a1", &line(2, "Grep"));
    assert_eq!(get(&table, "a1").state, TaskState::Stopped);
    let reopened = table.agent_line("a1", &line(5, "Grep"));
    assert!(reopened.changed && reopened.stopped.is_empty());
    assert_eq!(get(&table, "a1").state, TaskState::ToolWait);
    assert_eq!(get(&table, "a1").steps, Some(3));
    let start = spool("SubagentStart", 6, json!({"agent_id": "a1"}));
    let ends = json!({"type": "user", "toolEndsTurn": true, "timestamp": at(7).to_rfc3339()});
    table.event(&start);
    assert_eq!(table.agent_line("a1", &ends).stopped, Vec::<String>::new());
    assert_eq!(get(&table, "a1").state, TaskState::Done);
}

#[test]
fn a_quiet_tick_stalls_agents_never_shells_and_reports_only_changes() {
    let mut table = BackgroundTasks::default();
    evidence(&mut table, 0, shell("b1", "toolu_b", "Run it"));
    evidence(&mut table, 0, agent("a1", "toolu_a"));
    table.agent_line("a1", &line(1, "Read"));
    // An agent waiting on its own nested agent runs as long as that one does.
    evidence(&mut table, 0, agent("a2", "toolu_c"));
    table.agent_line("a2", &line(1, "Agent"));
    let budget = Duration::from_secs(60);
    assert_eq!(table.reevaluate(at(30), budget), Change::default());
    assert!(table.reevaluate(at(100), budget).changed);
    assert_eq!(get(&table, "a1").state, TaskState::Stalled);
    assert_eq!(get(&table, "a2").state, TaskState::ToolWait);
    assert_eq!(get(&table, "b1").state, TaskState::Running);
    assert_eq!(table.blocking(), 1, "only a2 still holds the close");
    assert_eq!(table.reevaluate(at(200), budget), Change::default());
    table.agent_line("a1", &line(201, "Read"));
    assert_eq!(get(&table, "a1").state, TaskState::ToolWait);
    assert_eq!(table.blocking(), 2);
    // A last line that reads done stalls past the budget like one generating.
    let said = json!({"type": "assistant", "timestamp": at(202).to_rfc3339(),
        "message": {"role": "assistant", "content": [{"type": "text", "text": "DONE"}]}});
    table.agent_line("a1", &said);
    assert_eq!(get(&table, "a1").state, TaskState::Running);
    assert_eq!(table.reevaluate(at(250), budget), Change::default());
    assert!(table.reevaluate(at(300), budget).changed);
    assert_eq!(get(&table, "a1").state, TaskState::Stalled);
}

#[test]
fn a_teammate_is_never_an_entry_and_ids_are_bounded() {
    let mut table = BackgroundTasks::default();
    table.event(&spool("SubagentStart", 1, json!({"agent_id": "mate"})));
    let teammate = AgentMeta {
        teammate: Some("Drafter".to_owned()),
        ..AgentMeta::default()
    };
    assert!(table.meta("mate", &teammate).changed);
    table.event(&spool("SubagentStart", 2, json!({"agent_id": "mate"})));
    let long = "a".repeat(200);
    table.event(&spool("SubagentStart", 3, json!({ "agent_id": long })));
    table.event(&stop(4, &[(long.as_str(), "shell")]));
    assert!(table.tasks().is_empty(), "{:?}", table.tasks());
}

#[test]
fn startup_resume_clear_session_end_and_agents_killed_clear_compact_and_interrupts_keep() {
    let kept = [
        spool("SessionStart", 5, json!({"source": "compact"})),
        spool("SessionStart", 5, json!({})),
    ];
    let cleared = ["startup", "resume", "clear"]
        .map(|source| spool("SessionStart", 5, json!({ "source": source })));
    let fresh = || {
        let mut table = BackgroundTasks::default();
        table.event(&spool("SubagentStart", 1, json!({"agent_id": "a1"})));
        table
    };
    for event in &kept {
        let mut table = fresh();
        assert_eq!(table.event(event), Change::default(), "{event:?}");
    }
    let interrupt = RecordMark::Interrupted { at: at(5) };
    assert_eq!(fresh().mark(&interrupt), Change::default());
    for event in cleared.iter().chain([&spool("SessionEnd", 5, json!({}))]) {
        let mut table = fresh();
        assert!(
            table.event(event).changed && table.tasks().is_empty(),
            "{event:?}"
        );
    }
    let mut killed = fresh();
    let change = killed.mark(&RecordMark::AgentsKilled { at: at(5) });
    assert!(change.changed && change.stopped.is_empty() && killed.blocking() == 0);
}
