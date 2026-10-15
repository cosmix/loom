use super::{build_flow, fit_flow};
use crate::tokens::accumulate::{AgentState, StageAgents};
use crate::tokens::model::{
    AgentKind, ModelUsage, TokenUsage, BUCKET_SECS, MAX_FLOW_BYTES, MAX_MODEL_ROWS, OTHER_MODEL,
    PLAN_SERIES_LEN, STAGE_SERIES_LEN,
};
use crate::tokens::tests_fixture::usage;
use std::collections::BTreeMap;

const NOW: i64 = 1_788_523_229;
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

#[test]
fn rate_at_the_first_second_of_a_bucket_uses_the_shorter_span() {
    let stages = one_stage(vec![(
        "a1",
        agent(AgentKind::Subagent, Some(1), &[(2, 2710)]),
    )]);

    let flow = build_flow(&stages, NOW_BUCKET * BUCKET_SECS);

    assert_eq!(flow.stages[0].rate, usage(0, 2710 * 60 / 271, 0, 0));
    assert_eq!(flow.plan.rate.cache_write, 600);
}

fn modeled(entries: &[(&str, Option<&str>, TokenUsage)]) -> AgentState {
    let mut state = agent(AgentKind::Main, Some(1), &[]);
    state.by_model = entries
        .iter()
        .map(|(model, effort, total)| ModelUsage {
            model: (*model).to_owned(),
            effort: effort.map(str::to_owned),
            total: *total,
        })
        .collect();
    state
}

#[test]
fn models_merge_across_agents_and_sort() {
    let first = modeled(&[
        ("opus", Some("high"), usage(1, 0, 0, 9)),
        ("sonnet", None, usage(0, 0, 5, 20)),
    ]);
    let second = modeled(&[
        ("opus", Some("high"), usage(0, 0, 3, 1)),
        ("opus", None, usage(0, 0, 0, 10)),
        ("haiku", None, usage(0, 0, 0, 10)),
    ]);
    let stages = one_stage(vec![("main-a", first), ("main-b", second)]);

    let flow = build_flow(&stages, NOW);

    let keys: Vec<(&str, Option<&str>)> = flow
        .models
        .iter()
        .map(|entry| (entry.model.as_str(), entry.effort.as_deref()))
        .collect();
    assert_eq!(
        keys,
        [
            ("sonnet", None),
            ("opus", Some("high")),
            ("haiku", None),
            ("opus", None)
        ]
    );
    assert_eq!(flow.models[1].total, usage(1, 0, 3, 10));
}

#[test]
fn models_past_the_row_cap_fold_into_other_and_still_sum() {
    let entries: Vec<(String, TokenUsage)> = (0..MAX_MODEL_ROWS + 3)
        .map(|n| (format!("m{n:02}"), usage(0, 0, 0, 100 - n as u64)))
        .collect();
    let borrowed: Vec<(&str, Option<&str>, TokenUsage)> = entries
        .iter()
        .map(|(model, total)| (model.as_str(), None, *total))
        .collect();
    let mut state = modeled(&borrowed);
    state.by_model.push(ModelUsage {
        model: OTHER_MODEL.to_owned(),
        effort: None,
        total: usage(0, 0, 0, 1000),
    });
    let mut expected = TokenUsage::default();
    for entry in &state.by_model {
        expected.add(&entry.total);
    }
    let stages = one_stage(vec![("main-a", state)]);

    let flow = build_flow(&stages, NOW);

    // 22 kept entries (the old other entry folded away) plus the new other.
    assert_eq!(flow.models.len(), MAX_MODEL_ROWS - 1);
    let last = flow.models.last().expect("last");
    assert_eq!(
        (last.model.as_str(), last.effort.as_ref()),
        (OTHER_MODEL, None)
    );
    assert_eq!(
        flow.models[0].model, "m00",
        "the old other entry is folded away"
    );
    let mut sum = TokenUsage::default();
    for entry in &flow.models {
        sum.add(&entry.total);
    }
    assert_eq!(sum, expected);
}

#[test]
fn hit_series_rounds_half_up_and_leaves_empty_buckets_none() {
    let mut state = agent(AgentKind::Main, Some(1), &[]);
    state.buckets.insert(NOW_BUCKET, usage(1, 0, 3, 0));
    state.buckets.insert(NOW_BUCKET - 1, usage(3, 0, 1, 0));
    state.buckets.insert(NOW_BUCKET - 2, usage(0, 0, 0, 500));
    state.buckets.insert(NOW_BUCKET - 3, usage(0, 0, 7, 0));
    state.buckets.insert(NOW_BUCKET - 4, usage(3, 0, 5, 0));
    let stages = one_stage(vec![("main-a", state)]);

    let flow = build_flow(&stages, NOW);

    let hits = &flow.stages[0].hit_series;
    assert_eq!(hits.len(), STAGE_SERIES_LEN);
    assert_eq!(hits[STAGE_SERIES_LEN - 1], Some(75));
    assert_eq!(hits[STAGE_SERIES_LEN - 2], Some(25));
    assert_eq!(
        hits[STAGE_SERIES_LEN - 3],
        None,
        "output only has no input side"
    );
    assert_eq!(hits[STAGE_SERIES_LEN - 4], Some(100));
    assert_eq!(hits[STAGE_SERIES_LEN - 5], Some(63), "62.5 rounds up");
    assert_eq!(hits[0], None);
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
