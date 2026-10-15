use super::*;
use crate::tokens::model::{
    AgentFlow, AgentKind, ModelUsage, PlanFlow, QuotaLeft, QuotaProvider, StageFlow,
};

const SAFE: u64 = 9_007_199_254_740_991;

fn usage_of(value: u64) -> TokenUsage {
    TokenUsage {
        input: value,
        cache_write: value,
        cache_read: value,
        output: value,
    }
}

fn flow_of(value: u64, series: Vec<u64>) -> TokenFlow {
    let agent = AgentFlow {
        id: "main-01ed9f4c".into(),
        kind: AgentKind::Main,
        agent_type: Some("loom-software-engineer".into()),
        model: Some("claude-opus-5-5".into()),
        active: true,
        total: usage_of(value),
        rate: usage_of(value),
        series: series.clone(),
    };
    TokenFlow {
        observed_at: 1_788_523_200,
        bucket_secs: 30,
        plan: PlanFlow {
            total: usage_of(value),
            rate: usage_of(value),
            series: series.clone(),
        },
        stages: vec![StageFlow {
            stage_id: "server".into(),
            total: usage_of(value),
            rate: usage_of(value),
            series,
            hit_series: Vec::new(),
            agents: vec![agent],
        }],
        models: Vec::new(),
        quota_left: vec![QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left: Some(u32::MAX),
            lasts_to_reset: false,
        }],
        unreadable_lines: value,
    }
}

#[test]
fn a_flow_inside_every_bound_is_left_alone() {
    let mut flow = flow_of(SAFE, vec![SAFE; STAGE_SERIES_LEN]);
    flow.plan.series.truncate(PLAN_SERIES_LEN);
    let mut sanitized = flow.clone();

    sanitize_token_flow(&mut sanitized);

    assert_eq!(sanitized, flow);
}

#[test]
fn every_published_number_is_clamped_to_the_javascript_safe_range() {
    let mut flow = flow_of(u64::MAX, vec![u64::MAX, SAFE + 1, 7]);
    flow.observed_at = i64::MAX;
    flow.bucket_secs = i64::MIN;

    sanitize_token_flow(&mut flow);

    let stage = &flow.stages[0];
    let agent = &stage.agents[0];
    assert_eq!(flow.observed_at, SAFE as i64);
    assert_eq!(flow.bucket_secs, -(SAFE as i64));
    assert_eq!(flow.unreadable_lines, SAFE);
    for published in [
        flow.plan.total,
        flow.plan.rate,
        stage.total,
        stage.rate,
        agent.total,
        agent.rate,
    ] {
        assert_eq!(published, usage_of(SAFE));
    }
    for series in [&flow.plan.series, &stage.series, &agent.series] {
        assert_eq!(series, &[SAFE, SAFE, 7]);
    }
}

#[test]
fn series_keep_only_their_newest_values_in_order() {
    let long: Vec<u64> = (0..1000).collect();
    let mut flow = flow_of(1, long.clone());

    sanitize_token_flow(&mut flow);

    let newest = |len: usize| long[long.len() - len..].to_vec();
    assert_eq!(flow.plan.series, newest(PLAN_SERIES_LEN));
    assert_eq!(flow.stages[0].series, newest(STAGE_SERIES_LEN));
    assert_eq!(flow.stages[0].agents[0].series, newest(STAGE_SERIES_LEN));
}

#[test]
fn shorter_series_are_not_padded() {
    let mut flow = flow_of(1, vec![5, 6]);

    sanitize_token_flow(&mut flow);

    assert_eq!(flow.plan.series, [5, 6]);
    assert_eq!(flow.stages[0].agents[0].series, [5, 6]);
}

#[test]
fn every_string_is_flattened() {
    let evil = |base: &str| format!("{base}\u{1b}[31m\u{202e}\nx");
    let mut flow = flow_of(1, vec![1]);
    let stage = &mut flow.stages[0];
    stage.stage_id = evil("server");
    let agent = &mut stage.agents[0];
    agent.id = evil("main");
    agent.agent_type = Some(evil("type"));
    agent.model = Some(evil("model"));

    sanitize_token_flow(&mut flow);

    let stage = &flow.stages[0];
    let agent = &stage.agents[0];
    let strings = [
        &stage.stage_id,
        &agent.id,
        agent.agent_type.as_ref().expect("agent_type"),
        agent.model.as_ref().expect("model"),
    ];
    for text in strings {
        assert!(!text.contains(['\u{1b}', '\u{202e}', '\n']), "{text:?}");
    }
}

fn model_entry(model: &str, effort: Option<&str>, value: u64) -> ModelUsage {
    ModelUsage {
        model: model.to_owned(),
        effort: effort.map(str::to_owned),
        total: usage_of(value),
    }
}

#[test]
fn model_entries_are_flattened_and_clamped() {
    let evil = |base: &str| format!("{base}\u{1b}[31m\u{202e}\nx");
    let mut flow = flow_of(1, vec![1]);
    flow.models = vec![
        model_entry(&evil("model"), Some(&evil("effort")), u64::MAX),
        model_entry("plain", None, 5),
    ];

    sanitize_token_flow(&mut flow);

    let first = &flow.models[0];
    let effort = first.effort.as_ref().expect("effort");
    for text in [&first.model, effort] {
        assert!(!text.contains(['\u{1b}', '\u{202e}', '\n']), "{text:?}");
    }
    assert_eq!(first.total, usage_of(SAFE));
    assert_eq!(flow.models[1], model_entry("plain", None, 5));
}

#[test]
fn models_are_cut_to_the_row_cap_keeping_the_first_entries() {
    let mut flow = flow_of(1, vec![1]);
    flow.models = (0..MAX_MODEL_ROWS + 5)
        .map(|n| model_entry(&format!("m{n}"), None, 1))
        .collect();

    sanitize_token_flow(&mut flow);

    assert_eq!(flow.models.len(), MAX_MODEL_ROWS);
    assert_eq!(flow.models[0].model, "m0");
    assert_eq!(flow.models[MAX_MODEL_ROWS - 1].model, "m23");
}

#[test]
fn hit_series_keeps_the_newest_values_and_caps_each_at_100() {
    let mut flow = flow_of(1, vec![1]);
    let mut hits: Vec<Option<u8>> = vec![Some(7); 30];
    hits.extend((0..STAGE_SERIES_LEN - 2).map(|_| None));
    hits.extend([Some(250), Some(100)]);
    flow.stages[0].hit_series = hits;

    sanitize_token_flow(&mut flow);

    let hit_series = &flow.stages[0].hit_series;
    assert_eq!(hit_series.len(), STAGE_SERIES_LEN);
    assert!(hit_series[..STAGE_SERIES_LEN - 2]
        .iter()
        .all(Option::is_none));
    assert_eq!(hit_series[STAGE_SERIES_LEN - 2..], [Some(100), Some(100)]);
}

#[test]
fn a_flow_is_fresh_from_now_back_to_the_stale_limit() {
    let now = 1_788_523_200;

    assert!(flow_is_fresh(now, now));
    assert!(flow_is_fresh(now - FLOW_STALE_SECS, now));
    assert!(!flow_is_fresh(now - FLOW_STALE_SECS - 1, now));
}

#[test]
fn a_future_flow_is_not_fresh() {
    let now = 1_788_523_200;

    assert!(!flow_is_fresh(now + 1, now));
    assert!(!flow_is_fresh(now + FLOW_STALE_SECS, now));
}

#[test]
fn extreme_timestamps_neither_overflow_nor_pass() {
    for observed_at in [i64::MIN, i64::MAX] {
        for now in [0, 1_788_523_200] {
            assert!(!flow_is_fresh(observed_at, now), "{observed_at} at {now}");
        }
    }
    assert!(!flow_is_fresh(i64::MIN, i64::MAX));
    assert!(!flow_is_fresh(i64::MAX, i64::MIN));
}
