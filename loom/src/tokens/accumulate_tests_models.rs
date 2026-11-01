use super::*;

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
