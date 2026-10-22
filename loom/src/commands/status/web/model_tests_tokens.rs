//! Fixture token-flow builders for [`super::fixture_snapshot`].
use crate::tokens::model::{
    AgentFlow, AgentKind, ModelUsage, PlanFlow, QuotaLeft, QuotaProvider, StageFlow, TokenFlow,
    TokenUsage, PLAN_SERIES_LEN, STAGE_SERIES_LEN,
};

fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

/// One agent of the token-flow fixture; `series` is `value` repeated 120 times.
fn fixture_agent(
    (id, kind, agent_type, model): (&str, AgentKind, Option<&str>, &str),
    total: TokenUsage,
    rate: TokenUsage,
    value: u64,
) -> AgentFlow {
    AgentFlow {
        id: id.to_owned(),
        kind,
        agent_type: agent_type.map(str::to_owned),
        model: Some(model.to_owned()),
        active: true,
        total,
        rate,
        series: vec![value; STAGE_SERIES_LEN],
    }
}

/// The pinned agents of the one stage in [`fixture_tokens`].
fn fixture_agents() -> Vec<AgentFlow> {
    vec![
        fixture_agent(
            ("main-01ed9f4c", AgentKind::Main, None, "claude-opus-5-5"),
            usage(40_000, 600_000, 9_000_000, 100_000),
            usage(600, 12_000, 200_000, 1_600),
            7_100,
        ),
        fixture_agent(
            (
                "a04baa183f888b773",
                AgentKind::Subagent,
                Some("loom-software-engineer"),
                "claude-sonnet-5-5",
            ),
            usage(50_000, 900_000, 13_000_000, 160_000),
            usage(1_200, 21_000, 350_000, 3_000),
            12_600,
        ),
        fixture_agent(
            ("codex-01a1396b", AgentKind::Codex, None, "gpt-6.1-sol"),
            usage(30_000, 300_000, 4_000_000, 50_000),
            usage(200, 3_000, 50_000, 600),
            1_900,
        ),
    ]
}

/// The plan-wide per (model, effort) totals; they sum to the plan total.
fn fixture_models() -> Vec<ModelUsage> {
    let entry = |model: &str, effort: &str, total: TokenUsage| ModelUsage {
        model: model.to_owned(),
        effort: Some(effort.to_owned()),
        total,
    };
    vec![
        entry(
            "claude-sonnet-5-5",
            "medium",
            usage(50_000, 900_000, 13_000_000, 160_000),
        ),
        entry(
            "claude-opus-5-5",
            "xhigh",
            usage(30_000, 450_000, 7_000_000, 80_000),
        ),
        entry(
            "gpt-6.1-sol",
            "xhigh",
            usage(30_000, 300_000, 4_000_000, 50_000),
        ),
        entry(
            "claude-opus-5-5",
            "high",
            usage(10_000, 150_000, 2_000_000, 20_000),
        ),
    ]
}

/// The pinned token-flow block behind [`fixture_snapshot`].
pub(super) fn fixture_tokens() -> TokenFlow {
    let total = usage(120_000, 1_800_000, 26_000_000, 310_000);
    let rate = usage(2_000, 36_000, 600_000, 5_200);
    TokenFlow {
        observed_at: 1_788_523_200,
        bucket_secs: 30,
        plan: PlanFlow {
            total,
            rate,
            series: vec![21_600; PLAN_SERIES_LEN],
        },
        stages: vec![StageFlow {
            stage_id: "server".to_owned(),
            total,
            rate,
            series: vec![21_600; STAGE_SERIES_LEN],
            hit_series: vec![Some(94); STAGE_SERIES_LEN],
            agents: fixture_agents(),
        }],
        models: fixture_models(),
        quota_left: vec![QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left: Some(127),
            lasts_to_reset: false,
        }],
        unreadable_lines: 0,
    }
}
