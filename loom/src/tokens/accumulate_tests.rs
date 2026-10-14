use super::{AgentState, MessageTable, MESSAGE_TABLE_CAP};
use crate::tokens::model::{AgentKind, TokenUsage, BUCKET_SECS};
use crate::tokens::tests_fixture::{claude_line, codex_count, usage};
use serde_json::json;

const NOW: i64 = 1_788_523_200;
const SONNET: &str = "claude-sonnet-5-5";

fn claude_agent() -> AgentState {
    AgentState::new(AgentKind::Main, "/t/session.jsonl")
}

#[test]
fn repeated_message_id_counts_once_and_adds_increases() {
    let mut agent = claude_agent();
    let first = usage(2, 14477, 4542, 20);
    agent.ingest(&claude_line("msg_A", NOW - 90, SONNET, &first));
    agent.ingest(&claude_line("msg_A", NOW - 90, SONNET, &first));
    agent.ingest(&claude_line(
        "msg_A",
        NOW - 89,
        SONNET,
        &usage(2, 14477, 4542, 50),
    ));

    assert_eq!(agent.total, usage(2, 14477, 4542, 50));
}

#[test]
fn usage_without_message_id_counts_each_time() {
    let mut agent = claude_agent();
    let line = json!({"type": "assistant", "timestamp": "2026-09-30T21:09:35.379Z",
        "message": {"usage": {"input_tokens": 2, "output_tokens": 20}}})
    .to_string();
    agent.ingest(&line);
    agent.ingest(&line);

    assert_eq!(agent.total, usage(4, 0, 0, 40));
}

#[test]
fn message_table_evicts_the_oldest_id_past_the_cap() {
    let mut table = MessageTable::default();
    let one = usage(1, 0, 0, 0);
    for n in 0..=MESSAGE_TABLE_CAP {
        table.increase(&format!("msg_{n}"), &one);
    }

    assert_eq!(table.max.len(), MESSAGE_TABLE_CAP);
    assert_eq!(table.order.front().map(String::as_str), Some("msg_1"));
    assert_eq!(
        table.increase("msg_0", &one),
        one,
        "an evicted id counts again"
    );
    assert_eq!(table.increase("msg_2", &one), TokenUsage::default());
}

#[test]
fn message_table_round_trips_in_order() {
    let mut table = MessageTable::default();
    table.increase("msg_b", &usage(1, 2, 3, 4));
    table.increase("msg_a", &usage(5, 6, 7, 8));

    let json = serde_json::to_value(&table).expect("serialize");
    let back: MessageTable = serde_json::from_value(json.clone()).expect("deserialize");

    assert_eq!(
        json,
        json!([["msg_b", [1, 2, 3, 4]], ["msg_a", [5, 6, 7, 8]]])
    );
    assert_eq!(back, table);
}

#[test]
fn backlog_lands_in_its_own_past_bucket() {
    let mut agent = claude_agent();
    agent.ingest(&claude_line(
        "msg_1",
        NOW - 600,
        SONNET,
        &usage(0, 1000, 0, 0),
    ));

    let now_bucket = NOW.div_euclid(BUCKET_SECS);
    let buckets: Vec<_> = agent.buckets.keys().copied().collect();
    assert_eq!(buckets, [now_bucket - 20]);
    assert_eq!(agent.first_seen, Some(NOW - 600));
}

#[test]
fn codex_counts_deltas_and_restarts_on_a_decrease() {
    let mut agent = AgentState::new(AgentKind::Codex, "/r/rollout.jsonl");
    agent.ingest(&codex_count(NOW - 60, [21255, 12160, 110]));
    agent.ingest(&codex_count(NOW - 50, [43274, 33280, 206]));
    assert_eq!(agent.total, usage(9994, 0, 33280, 206));

    agent.ingest(&codex_count(NOW - 40, [1000, 400, 10]));

    assert_eq!(agent.total, usage(10594, 0, 33680, 216));
}

#[test]
fn codex_model_is_the_last_one_seen_and_flattened() {
    let mut agent = AgentState::new(AgentKind::Codex, "/r/rollout.jsonl");
    let context = |model: &str| json!({"type": "turn_context", "payload": {"model": model}});
    agent.ingest(&context("gpt-6.1-sol").to_string());
    agent.ingest(&context("gpt\u{1b}[31m\nx").to_string());

    let model = agent.model.expect("model");
    assert!(!model.contains(['\u{1b}', '\n']), "model {model:?}");
}

#[test]
fn unreadable_usage_lines_are_counted() {
    let mut claude = claude_agent();
    let bad = claude_line("msg_1", NOW, SONNET, &usage(1, 2, 3, 4))
        .replace("\"output_tokens\":4", "\"output_tokens\":\"many\"");
    claude.ingest(&bad);
    claude.ingest("{\"usage\": torn");
    claude.ingest(&json!({"type": "user", "message": {"content": "usage?"}}).to_string());
    let mut codex = AgentState::new(AgentKind::Codex, "/r/rollout.jsonl");
    codex.ingest("{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\"");

    assert_eq!(claude.unreadable, 2);
    assert_eq!(codex.unreadable, 1);
    assert_eq!(claude.total, TokenUsage::default());
}

#[test]
fn restart_forgets_the_file_but_keeps_the_agent_type() {
    let mut agent = AgentState::new(AgentKind::Subagent, "/t/agent-a1.jsonl");
    agent.agent_type = Some("loom-software-engineer".into());
    agent.ingest(&claude_line("msg_1", NOW, SONNET, &usage(1, 2, 3, 4)));
    agent.file.offset = 512;

    agent.restart("/t/agent-a1.jsonl", 77);

    let mut expected = AgentState::new(AgentKind::Subagent, "/t/agent-a1.jsonl");
    expected.agent_type = Some("loom-software-engineer".into());
    expected.file.inode = 77;
    assert_eq!(agent, expected);
}

#[test]
fn prune_keeps_totals() {
    let mut agent = claude_agent();
    agent.ingest(&claude_line(
        "msg_1",
        NOW - 7200,
        SONNET,
        &usage(0, 10, 0, 0),
    ));
    agent.ingest(&claude_line("msg_2", NOW, SONNET, &usage(0, 5, 0, 0)));

    agent.prune(NOW.div_euclid(BUCKET_SECS) - 119);

    assert_eq!(agent.buckets.len(), 1);
    assert_eq!(agent.total.cache_write, 15);
}
