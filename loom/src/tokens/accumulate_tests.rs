use super::{AgentState, MessageTable, MESSAGE_TABLE_CAP};
use crate::tokens::model::{
    AgentKind, TokenUsage, BUCKET_SECS, MAX_MODEL_KEYS, OTHER_MODEL, UNKNOWN_MODEL,
};
use crate::tokens::tests_fixture::{claude_line, claude_line_effort, codex_count, usage};
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

#[test]
fn synthetic_error_rows_do_not_replace_the_model() {
    let mut agent = claude_agent();
    agent.ingest(&claude_line("msg_1", NOW - 60, SONNET, &usage(1, 2, 3, 4)));
    agent.ingest(&claude_line(
        "msg_2",
        NOW - 30,
        "<synthetic>",
        &TokenUsage::default(),
    ));

    assert_eq!(agent.model.as_deref(), Some(SONNET));
    assert_eq!(agent.total, usage(1, 2, 3, 4));

    let mut only_errors = claude_agent();
    only_errors.ingest(&claude_line(
        "msg_3",
        NOW,
        "<synthetic>",
        &TokenUsage::default(),
    ));
    assert_eq!(only_errors.model, None);
}

fn model_key(agent: &AgentState, model: &str, effort: Option<&str>) -> TokenUsage {
    agent
        .by_model
        .iter()
        .find(|entry| entry.model == model && entry.effort.as_deref() == effort)
        .map_or_else(TokenUsage::default, |entry| entry.total)
}

fn by_model_sum(agent: &AgentState) -> TokenUsage {
    let mut sum = TokenUsage::default();
    for entry in &agent.by_model {
        sum.add(&entry.total);
    }
    sum
}

#[test]
fn usage_is_split_by_the_effort_of_each_line() {
    let mut agent = claude_agent();
    agent.ingest(&claude_line_effort(
        "m1",
        NOW,
        SONNET,
        "high",
        &usage(1, 2, 3, 4),
    ));
    agent.ingest(&claude_line_effort(
        "m2",
        NOW,
        SONNET,
        "low",
        &usage(10, 20, 30, 40),
    ));

    assert_eq!(agent.by_model.len(), 2);
    assert_eq!(model_key(&agent, SONNET, Some("high")), usage(1, 2, 3, 4));
    assert_eq!(
        model_key(&agent, SONNET, Some("low")),
        usage(10, 20, 30, 40)
    );
    assert_eq!(by_model_sum(&agent), agent.total);
}

#[test]
fn a_line_without_effort_inherits_the_last_one() {
    let mut agent = claude_agent();
    agent.ingest(&claude_line_effort(
        "m1",
        NOW,
        SONNET,
        "high",
        &usage(1, 2, 3, 4),
    ));
    agent.ingest(&claude_line("m2", NOW, SONNET, &usage(1, 1, 1, 1)));

    assert_eq!(agent.by_model.len(), 1);
    assert_eq!(model_key(&agent, SONNET, Some("high")), usage(2, 3, 4, 5));
}

#[test]
fn usage_before_any_model_is_unknown() {
    let mut agent = claude_agent();
    let line = json!({"type": "assistant", "timestamp": "2026-09-30T21:09:35.379Z",
        "message": {"usage": {"input_tokens": 2, "output_tokens": 20}}})
    .to_string();
    agent.ingest(&line);

    assert_eq!(model_key(&agent, UNKNOWN_MODEL, None), usage(2, 0, 0, 20));
}

#[test]
fn model_keys_are_capped_and_overflow_goes_to_other() {
    let mut agent = claude_agent();
    for n in 0..10 {
        let model = format!("model-{n}");
        agent.ingest(&claude_line(
            &format!("m{n}"),
            NOW,
            &model,
            &usage(1, 0, 0, 1),
        ));
    }

    assert_eq!(agent.by_model.len(), MAX_MODEL_KEYS + 1);
    assert_eq!(model_key(&agent, OTHER_MODEL, None), usage(2, 0, 0, 2));
    assert_eq!(by_model_sum(&agent), agent.total);
    assert_eq!(agent.total, usage(10, 0, 0, 10));
}

#[test]
fn codex_turn_context_effort_keys_the_usage() {
    let mut agent = AgentState::new(AgentKind::Codex, "/r/rollout.jsonl");
    let context = json!({"type": "turn_context",
        "payload": {"model": "gpt-6.1-sol", "effort": "xhigh"}});
    agent.ingest(&context.to_string());
    agent.ingest(&codex_count(NOW, [100, 40, 10]));

    assert_eq!(agent.effort.as_deref(), Some("xhigh"));
    assert_eq!(
        model_key(&agent, "gpt-6.1-sol", Some("xhigh")),
        usage(60, 0, 40, 10)
    );
}

#[test]
fn codex_turn_context_without_effort_clears_the_previous_one() {
    let mut agent = AgentState::new(AgentKind::Codex, "/r/rollout.jsonl");
    let with_effort = json!({"type": "turn_context",
        "payload": {"model": "gpt-6.1-sol", "effort": "xhigh"}});
    agent.ingest(&with_effort.to_string());
    agent.ingest(&codex_count(NOW, [100, 40, 10]));
    let without_effort = json!({"type": "turn_context",
        "payload": {"model": "gpt-6.1-sol"}});
    agent.ingest(&without_effort.to_string());
    agent.ingest(&codex_count(NOW + 10, [150, 40, 15]));

    assert_eq!(agent.effort, None);
    assert_eq!(
        model_key(&agent, "gpt-6.1-sol", Some("xhigh")),
        usage(60, 0, 40, 10)
    );
    assert_eq!(model_key(&agent, "gpt-6.1-sol", None), usage(50, 0, 0, 5));
}

fn sonnet_effort_line(id: &str, effort: &str, spent: &TokenUsage) -> String {
    claude_line_effort(id, NOW, SONNET, effort, spent)
}

fn turn_context(model: &str, effort: &str) -> String {
    json!({"type": "turn_context", "payload": {"model": model, "effort": effort}}).to_string()
}

#[test]
fn a_claude_effort_that_sanitizes_to_nothing_leaves_the_current_one() {
    let mut agent = claude_agent();
    agent.ingest(&sonnet_effort_line("m1", "high", &usage(1, 2, 3, 4)));
    agent.ingest(&sonnet_effort_line("m2", " \t\n ", &usage(1, 1, 1, 1)));

    assert_eq!(agent.effort.as_deref(), Some("high"));
    assert_eq!(model_key(&agent, SONNET, Some("high")), usage(2, 3, 4, 5));
}

#[test]
fn a_claude_model_that_sanitizes_to_nothing_leaves_the_current_one() {
    let mut agent = claude_agent();
    agent.ingest(&claude_line("m1", NOW, SONNET, &usage(1, 2, 3, 4)));
    agent.ingest(&claude_line("m2", NOW, "  \n ", &usage(1, 1, 1, 1)));

    assert_eq!(agent.model.as_deref(), Some(SONNET));
    assert_eq!(model_key(&agent, SONNET, None), usage(2, 3, 4, 5));

    let mut bare = claude_agent();
    bare.ingest(&claude_line("m3", NOW, "\t", &usage(1, 0, 0, 1)));
    assert_eq!(bare.model, None);
    assert_eq!(model_key(&bare, UNKNOWN_MODEL, None), usage(1, 0, 0, 1));
}

#[test]
fn a_codex_model_or_effort_that_sanitizes_to_nothing_is_absent() {
    let mut no_effort = AgentState::new(AgentKind::Codex, "/r/rollout.jsonl");
    no_effort.ingest(&turn_context("gpt-6.1-sol", "  \n"));
    no_effort.ingest(&codex_count(NOW, [100, 40, 10]));
    assert_eq!(no_effort.effort, None);
    assert_eq!(
        model_key(&no_effort, "gpt-6.1-sol", None),
        usage(60, 0, 40, 10)
    );

    let mut no_model = AgentState::new(AgentKind::Codex, "/r/rollout.jsonl");
    no_model.ingest(&turn_context(" \t ", "xhigh"));
    no_model.ingest(&codex_count(NOW, [100, 40, 10]));
    assert_eq!(no_model.model, None);
    assert_eq!(
        model_key(&no_model, UNKNOWN_MODEL, Some("xhigh")),
        usage(60, 0, 40, 10)
    );
}

#[test]
fn restart_clears_the_model_split() {
    let mut agent = claude_agent();
    agent.ingest(&claude_line_effort(
        "m1",
        NOW,
        SONNET,
        "high",
        &usage(1, 2, 3, 4),
    ));

    agent.restart("/t/session.jsonl", 5);

    assert!(agent.by_model.is_empty());
    assert_eq!(agent.effort, None);
}

#[test]
fn skipped_line_counts_as_unreadable() {
    let mut agent = claude_agent();
    agent.count_unreadable();
    agent.count_unreadable();

    assert_eq!(agent.unreadable, 2);
}

#[test]
fn unreadable_count_saturates_instead_of_overflowing() {
    let mut agent = claude_agent();
    agent.unreadable = u64::MAX;
    agent.count_unreadable();

    assert_eq!(agent.unreadable, u64::MAX);
}
