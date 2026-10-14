use super::{build_flow, fit_flow};
use crate::tokens::accumulate::{AgentState, StageAgents};
use crate::tokens::model::{
    AgentKind, TokenUsage, BUCKET_SECS, MAX_FLOW_BYTES, PLAN_SERIES_LEN, STAGE_SERIES_LEN,
};
use crate::tokens::tests_fixture::usage;
use std::collections::BTreeMap;

const NOW: i64 = 1_788_523_200;
const NOW_BUCKET: i64 = NOW / BUCKET_SECS;

/// An agent with `cache_write` usage in buckets `back` buckets before now.
fn agent(kind: AgentKind, first_seen: Option<i64>, spent: &[(i64, u64)]) -> AgentState {
    let mut state = AgentState::new(kind, "/t/file.jsonl");
    state.first_seen = first_seen;
    for (back, tokens) in spent {
        let bucket_usage = usage(0, *tokens, 0, 0);
        state.total.add(&bucket_usage);
        state.buckets.insert(NOW_BUCKET - back, bucket_usage);
    }
    state
}

fn one_stage(agents: Vec<(&str, AgentState)>) -> BTreeMap<String, StageAgents> {
    let agents = agents
        .into_iter()
        .map(|(key, state)| (key.to_owned(), state))
        .collect();
    BTreeMap::from([("alpha".to_owned(), agents)])
}

#[test]
fn rate_series_and_activity_follow_the_buckets() {
    let stages = one_stage(vec![
        (
            "main-01ed9f4c-aaaa",
            agent(AgentKind::Main, Some(1), &[(14, 6000), (2, 3000)]),
        ),
        ("a1", agent(AgentKind::Subagent, Some(2), &[(4, 500)])),
    ]);

    let flow = build_flow(&stages, NOW);

    let alpha = &flow.stages[0];
    assert_eq!(alpha.rate, usage(0, 700, 0, 0));
    assert_eq!(alpha.agents[0].rate, usage(0, 600, 0, 0));
    assert!(alpha.agents[0].active);
    assert!(!alpha.agents[1].active, "usage 4 buckets back is idle");
    assert_eq!(alpha.series.len(), STAGE_SERIES_LEN);
    assert_eq!(alpha.series[STAGE_SERIES_LEN - 15], 6000);
    assert_eq!(flow.plan.series.len(), PLAN_SERIES_LEN);
    assert_eq!(flow.plan.series[PLAN_SERIES_LEN - 3], 3000);
    assert_eq!(flow.plan.total.cache_write, 9500);
}

#[test]
fn agents_order_by_kind_then_first_seen() {
    let stages = one_stage(vec![
        ("codex-01a1396b-2aa4", agent(AgentKind::Codex, Some(1), &[])),
        ("b2", agent(AgentKind::Subagent, None, &[])),
        ("a1", agent(AgentKind::Subagent, Some(9), &[])),
        ("main-02ed9f4c-aaaa", agent(AgentKind::Main, Some(5), &[])),
        ("main-01ed9f4c-aaaa", agent(AgentKind::Main, Some(7), &[])),
    ]);

    let flow = build_flow(&stages, NOW);

    let ids: Vec<&str> = flow.stages[0]
        .agents
        .iter()
        .map(|a| a.id.as_str())
        .collect();
    let expected = [
        "main-02ed9f4c",
        "main-01ed9f4c",
        "a1",
        "b2",
        "codex-01a1396b",
    ];
    assert_eq!(ids, expected);
}

#[test]
fn shared_short_ids_fall_back_to_full_keys() {
    let stages = one_stage(vec![
        (
            "codex-01a1396b-2aa4-7d93",
            agent(AgentKind::Codex, Some(1), &[]),
        ),
        (
            "codex-01a1396b-9999-7d93",
            agent(AgentKind::Codex, Some(2), &[]),
        ),
    ]);

    let flow = build_flow(&stages, NOW);

    let ids: Vec<&str> = flow.stages[0]
        .agents
        .iter()
        .map(|a| a.id.as_str())
        .collect();
    assert_eq!(
        ids,
        ["codex-01a1396b-2aa4-7d93", "codex-01a1396b-9999-7d93"]
    );
}

#[test]
fn plan_sums_stages_and_unreadable_lines() {
    let mut stages = one_stage(vec![(
        "a1",
        agent(AgentKind::Subagent, Some(1), &[(0, 50)]),
    )]);
    let mut beta = agent(AgentKind::Main, Some(1), &[(0, 100)]);
    beta.unreadable = 2;
    stages.insert("beta".into(), BTreeMap::from([("main-x".to_owned(), beta)]));

    let flow = build_flow(&stages, NOW);

    assert_eq!(flow.observed_at, NOW);
    assert_eq!(flow.bucket_secs, BUCKET_SECS);
    assert_eq!(flow.plan.total, usage(0, 150, 0, 0));
    assert_eq!(flow.plan.series[PLAN_SERIES_LEN - 1], 150);
    assert_eq!(flow.unreadable_lines, 2);
    let ids: Vec<&str> = flow.stages.iter().map(|s| s.stage_id.as_str()).collect();
    assert_eq!(ids, ["alpha", "beta"]);
}

/// `count` subagents, each with one bucket `back` buckets before now.
fn many_subagents(count: usize, back: i64) -> BTreeMap<String, StageAgents> {
    let agents = (0..count)
        .map(|n| {
            let state = agent(AgentKind::Subagent, Some(1), &[(back, 100)]);
            (format!("a{n:016x}"), state)
        })
        .collect();
    BTreeMap::from([("alpha".to_owned(), agents)])
}

#[test]
fn oversized_flow_sheds_idle_series_only() {
    let mut stages = many_subagents(1600, 60);
    let lead = agent(AgentKind::Main, Some(1), &[(1, 10)]);
    stages
        .get_mut("alpha")
        .expect("alpha")
        .insert("main-x".into(), lead);
    let mut flow = build_flow(&stages, NOW);

    let json = fit_flow(&mut flow)
        .expect("serialize")
        .expect("fits after shedding");

    assert!(json.len() as u64 <= MAX_FLOW_BYTES);
    let agents = &flow.stages[0].agents;
    assert_eq!(
        agents[0].series.len(),
        STAGE_SERIES_LEN,
        "the active agent keeps its series"
    );
    assert!(agents[1..]
        .iter()
        .all(|a| a.series.is_empty() && a.total.cache_write == 100));
}

#[test]
fn flow_still_over_the_cap_is_refused() {
    let mut flow = build_flow(&many_subagents(2000, 0), NOW);

    assert_eq!(fit_flow(&mut flow).expect("serialize"), None);
}

#[test]
fn small_flow_is_serialized_whole() {
    let mut flow = build_flow(&many_subagents(2, 60), NOW);
    let before = flow.clone();

    let json = fit_flow(&mut flow).expect("serialize").expect("fits");

    assert_eq!(flow, before);
    assert_eq!(json, serde_json::to_string(&before).expect("serialize"));
    assert_eq!(flow.stages[0].rate, TokenUsage::default());
}
