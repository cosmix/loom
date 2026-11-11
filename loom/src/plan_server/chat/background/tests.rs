//! The background frame's join, its change detection and the thread behind it.

use std::fs;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::{json, Value};

use super::groups::chat_groups;
use super::state::Board;
use super::{frame, PlanMessage};
use crate::plan_server::chat::tests::{browser, Fixture};
use crate::plan_session::bridge::background::{BackgroundTask, TaskKind, TaskState};
use crate::tokens::{AgentFlow, AgentKind, TokenUsage};

fn task(id: &str, kind: TaskKind, state: TaskState) -> BackgroundTask {
    let at = Utc::now();
    BackgroundTask {
        id: id.to_owned(),
        kind,
        label: Some(format!("task {id}")),
        agent_type: None,
        tool_use_id: Some(format!("toolu_{id}")),
        state,
        started_at: at,
        last_activity_at: at,
        steps: None,
        last_tool: None,
        report_preview: None,
    }
}

fn flow(id: &str, total: u64, active: bool) -> AgentFlow {
    let usage = |output| TokenUsage {
        input: 100,
        cache_write: 10,
        cache_read: 5000,
        output,
    };
    AgentFlow {
        id: id.to_owned(),
        kind: AgentKind::Subagent,
        agent_type: None,
        forwarder: None,
        model: Some("claude-haiku-5".to_owned()),
        active,
        total: usage(total),
        rate: usage(7),
        series: Vec::new(),
    }
}

/// The frames a board sent, as JSON.
#[derive(Default)]
struct Sent(Mutex<Vec<Value>>);

impl Sent {
    fn emit(&self) -> impl Fn(&PlanMessage) + '_ {
        |message| {
            self.0
                .lock()
                .unwrap()
                .push(serde_json::to_value(message).unwrap())
        }
    }
    fn all(&self) -> Vec<Value> {
        self.0.lock().unwrap().clone()
    }
}

fn agent(id: &str) -> BackgroundTask {
    task(id, TaskKind::Agent, TaskState::Running)
}

fn pass(session: &str, rows: Vec<AgentFlow>) -> Vec<(String, Vec<AgentFlow>)> {
    vec![(session.to_owned(), rows)]
}

#[test]
fn a_task_takes_the_fresh_figures_of_the_row_with_its_id_and_a_shell_none() {
    let tasks = [
        agent("a1"),
        task("sh1", TaskKind::Shell, TaskState::Running),
    ];
    let value = serde_json::to_value(frame("s1", &tasks, &[flow("a1", 40, true)])).unwrap();
    let (agent_row, shell_row) = (&value["tasks"][0], &value["tasks"][1]);
    assert_eq!(agent_row["total"], 150);
    assert_eq!(agent_row["tokens_per_min"], 117);
    assert_eq!(agent_row["active"], true);
    assert_eq!(agent_row["model"], "claude-haiku-5");
    for key in ["total", "tokens_per_min", "active", "model"] {
        assert_eq!(shell_row[key], Value::Null, "{key}");
    }
}

#[test]
fn repeating_the_last_tasks_broadcasts_nothing() {
    let (board, sent) = (Board::default(), Sent::default());
    let tasks = vec![agent("a1")];
    board.tasks("s1", tasks.clone(), &sent.emit());
    board.tasks("s1", tasks, &sent.emit());
    assert_eq!(sent.all().len(), 1, "{:?}", sent.all());
}

#[test]
fn a_chat_with_no_tasks_says_nothing_until_it_has_some() {
    let (board, sent) = (Board::default(), Sent::default());
    board.tasks("s1", Vec::new(), &sent.emit());
    assert!(sent.all().is_empty());
    assert!(board.replay().is_none());
}

#[test]
fn an_ended_chat_broadcasts_the_empty_frame_and_is_forgotten() {
    let (board, sent) = (Board::default(), Sent::default());
    board.tasks("s1", vec![agent("a1")], &sent.emit());
    board.ended("other", &sent.emit());
    assert_eq!(sent.all().len(), 1, "another session's end is not ours");
    board.ended("s1", &sent.emit());
    let last = sent.all().pop().unwrap();
    assert_eq!(
        last,
        json!({"type": "background", "session": "s1", "tasks": []})
    );
    assert!(board.replay().is_none());
}

#[test]
fn a_pass_that_moves_only_an_active_total_broadcasts_the_same_table() {
    let (board, sent) = (Board::default(), Sent::default());
    board.tasks("s1", vec![agent("a1")], &sent.emit());
    board.flows(pass("s1", vec![flow("a1", 40, true)]), &sent.emit());
    board.flows(pass("s1", vec![flow("a1", 40, true)]), &sent.emit());
    board.flows(pass("s1", vec![flow("a1", 90, true)]), &sent.emit());
    let frames = sent.all();
    assert_eq!(frames.len(), 3, "{frames:?}");
    assert_eq!(frames[2]["tasks"][0]["total"], 200);
    let table = |frame: &Value| frame["tasks"][0]["id"].clone();
    assert_eq!(table(&frames[0]), table(&frames[2]));
    assert_eq!(frames[0]["tasks"][0]["total"], Value::Null);
}

#[test]
fn a_pass_over_an_empty_table_broadcasts_nothing() {
    let (board, sent) = (Board::default(), Sent::default());
    board.flows(pass("s1", vec![flow("a1", 40, true)]), &sent.emit());
    board.tasks("s1", Vec::new(), &sent.emit());
    board.flows(pass("s1", vec![flow("a1", 90, true)]), &sent.emit());
    assert!(sent.all().is_empty());
}

#[test]
fn rows_read_before_the_first_table_are_joined_when_it_arrives() {
    let (board, sent) = (Board::default(), Sent::default());
    board.flows(pass("s1", vec![flow("a1", 40, false)]), &sent.emit());
    board.tasks("s1", vec![agent("a1")], &sent.emit());
    assert_eq!(sent.all()[0]["tasks"][0]["total"], 150);
}

#[test]
fn a_new_chat_session_empties_the_old_sessions_frame_first() {
    let (board, sent) = (Board::default(), Sent::default());
    board.tasks("s1", vec![agent("a1")], &sent.emit());
    board.flows(pass("s1", vec![flow("a1", 40, true)]), &sent.emit());
    board.tasks("s2", vec![agent("b1")], &sent.emit());
    let frames = sent.all();
    let [.., emptied, fresh] = frames.as_slice() else {
        panic!("{frames:?}")
    };
    assert_eq!(
        emptied,
        &json!({"type": "background", "session": "s1", "tasks": []})
    );
    assert_eq!(fresh["session"], "s2");
    assert_eq!(
        fresh["tasks"][0]["total"],
        Value::Null,
        "s1's rows are not s2's"
    );
}

#[test]
fn a_new_tab_gets_the_replay_of_the_last_frame() {
    let f = Fixture::new(false);
    let board = f.ctx.extensions().get::<Board>().expect("the board");
    let first = f.connect(&browser());
    assert!(first
        .opened
        .iter()
        .all(|frame| frame["type"] != "background"));
    let ctx = f.ctx.clone();
    board.tasks("s1", vec![agent("a1")], &|m| ctx.hub().broadcast(m));
    let later = f.connect(&browser());
    let replay: Vec<&Value> = later
        .opened
        .iter()
        .filter(|m| m["type"] == "background")
        .collect();
    assert_eq!(replay.len(), 1, "{:?}", later.opened);
    assert_eq!(replay[0]["session"], "s1");
    assert_eq!(replay[0]["tasks"][0]["id"], "a1");
}

#[test]
fn the_group_holds_the_chat_alone_skips_teammates_and_takes_the_metas_agent_type() {
    let f = Fixture::new(false);
    assert!(chat_groups(&f.bridge).is_empty(), "no chat, no group");
    let s = f.idle_chat(&f.connect(&browser()));
    let main = f.bridge.transcripts(&s.id).and_then(|t| t.main).unwrap();
    let dir = main.with_extension("").join("subagents");
    fs::create_dir_all(&dir).unwrap();
    let metas = [
        (
            "a1",
            json!({"agentType": "Explore", "toolUseId": "toolu_1"}),
        ),
        (
            "t1",
            json!({"agentType": "mate", "taskKind": "in_process_teammate"}),
        ),
        ("a2", json!({"toolUseId": "toolu_2"})),
    ];
    for (id, meta) in metas {
        fs::write(dir.join(format!("agent-{id}.meta.json")), meta.to_string()).unwrap();
        fs::write(dir.join(format!("agent-{id}.jsonl")), "").unwrap();
    }
    let groups = chat_groups(&f.bridge);
    assert_eq!(groups.len(), 1);
    assert_eq!(
        (groups[0].id.as_str(), &groups[0].main),
        (s.id.as_str(), &main)
    );
    let mut agents: Vec<(&str, Option<&str>)> = groups[0]
        .subagents
        .iter()
        .map(|(id, _, kind)| (id.as_str(), kind.as_deref()))
        .collect();
    agents.sort();
    assert_eq!(agents, [("a1", Some("Explore")), ("a2", None)]);
}

#[test]
fn shutdown_joins_the_planner_tokens_thread() {
    let f = Fixture::new(false);
    let started = Instant::now();
    f.ctx.stop_workers().expect("workers stop and join");
    assert!(started.elapsed() < Duration::from_secs(10));
}
