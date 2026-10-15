//! Contracts for the plan rollup, the flow size cap and quota reset splitting.

#[allow(dead_code)]
#[path = "token_flow_contracts/support.rs"]
mod support;

use loom::tokens::{
    flow_path, read_flow, AgentKind, QuotaLeft, QuotaProvider, TokenUsage, BUCKET_SECS,
    MAX_FLOW_BYTES, PLAN_SERIES_LEN, STAGE_SERIES_LEN,
};
use std::fs;
use support::*;

#[test]
fn plan_rate_sums_stages_and_expires() {
    let fx = Fx::new();
    let now = unix_now().div_euclid(BUCKET_SECS) * BUCKET_SECS - 1;
    let alpha = claude_line("msg_a", now - 60, &usage(0, 3000, 0, 0));
    let beta = claude_line("msg_b", now - 60, &usage(0, 1500, 0, 0));
    fx.add_stage("alpha", &uuid(1), &body(&[alpha]));
    fx.add_stage("beta", &uuid(2), &body(&[beta]));

    let flow = fx.scan(now);
    assert_eq!(flow.plan.rate, usage(0, 900, 0, 0));
    assert_eq!(flow.plan.total.cache_write, 4500);
    assert_eq!(flow.plan.series.len(), PLAN_SERIES_LEN);
    assert_eq!(flow.plan.series[PLAN_SERIES_LEN - 3], 4500);
    for id in ["alpha", "beta"] {
        assert_eq!(stage(&flow, id).series.len(), STAGE_SERIES_LEN);
        assert!(main_agent(stage(&flow, id)).active, "{id} agent inactive");
    }

    let later = fx.scan(now + 600);
    assert_eq!(later.plan.rate, TokenUsage::default());
    assert_eq!(later.plan.total.cache_write, 4500);
    assert_eq!(later.plan.series[PLAN_SERIES_LEN - 23], 4500);
    for id in ["alpha", "beta"] {
        assert_eq!(stage(&later, id).rate, TokenUsage::default(), "{id} rate");
        assert!(
            !main_agent(stage(&later, id)).active,
            "{id} agent still active"
        );
    }
}

#[test]
fn oversized_flow_sheds_idle_series() {
    let fx = Fx::new();
    let now = unix_now();
    let main = claude_line("msg_main", now - 60, &usage(0, 0, 0, 10));
    fx.add_stage("alpha", &uuid(1), &body(&[main]));
    for i in 0..1600u32 {
        let agent = format!("a{i:016x}");
        let msg = format!("msg_sub_{i}");
        let line = subagent_line(
            &agent,
            &msg,
            now - 1800,
            "claude-sonnet-5-5",
            &usage(0, 100, 0, 0),
        );
        fx.add_subagent(&uuid(1), &agent, &body(&[line]));
    }
    fx.scan(now);

    let size = fs::metadata(flow_path(&fx.work_root))
        .expect("flow.json")
        .len();
    assert!(size <= MAX_FLOW_BYTES, "flow.json is {size} bytes");
    let flow = read_flow(&fx.work_root).expect("read_flow");
    let alpha = stage(&flow, "alpha");
    assert_eq!(alpha.agents.len(), 1601);
    assert_eq!(flow.plan.total.cache_write, 160_000);
    assert_eq!(flow.plan.total.output, 10);
    for sub in agents_of(alpha, AgentKind::Subagent) {
        assert!(
            !sub.active && sub.series.is_empty(),
            "idle {} kept series",
            sub.id
        );
    }
    let lead = main_agent(alpha);
    assert!(lead.active);
    assert_eq!(lead.series.len(), STAGE_SERIES_LEN);
    let tokens = fx
        .status_tokens()
        .expect("collector publishes the shed flow");
    assert_eq!(stage(&tokens, "alpha").agents.len(), 1601);
}

#[test]
fn quota_left_ignores_pre_reset_points() {
    let fx = Fx::new();
    let now = unix_now();
    let week = |used| Some((used, now + 400_000));
    fx.write_claude_history(&[
        history_row(now - 1500, (90.0, now - 600), week(40.0)),
        history_row(now - 1200, (95.0, now - 600), week(40.5)),
        history_row(now - 630, (5.0, now + 17300), None),
        history_row(now - 30, (35.0, now + 17300), None),
    ]);

    let flow = fx.scan(now);

    let expected = QuotaLeft {
        provider: QuotaProvider::Claude,
        minutes_left: Some(21),
        lasts_to_reset: false,
    };
    assert_eq!(flow.quota_left, vec![expected]);
}
