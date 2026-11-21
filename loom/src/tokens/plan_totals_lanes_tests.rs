//! A Claude transcript and a Codex rollout of one stage, through the final
//! scan into the plan totals: one model row per lane, the totals their sum.
use serde_json::json;
use std::collections::BTreeSet;

use crate::tokens::model::TokenUsage;
use crate::tokens::plan_totals::{plan_tokens, PlanTokens, CLAUDE_LANE, CODEX_LANE};
use crate::tokens::tests_fixture::{
    body, claude_line, codex_count, unix_now, usage, Fixture, SESSION_UUID, THREAD_ID,
};
use crate::tokens::tracker::final_scan;

fn rollout(ts: i64) -> String {
    let context = json!({"type": "turn_context", "payload": {
        "model": "gpt-6.1-terra", "effort": "xhigh"}});
    format!("{context}\n{}\n", codex_count(ts, [50, 20, 7]))
}

/// A stage `alpha` with one Claude transcript and one forwarded Codex thread.
fn both_lanes(now: i64) -> Fixture {
    let fx = Fixture::new();
    let claude = claude_line("m1", now - 90, "claude-opus-5-5", &usage(10, 100, 0, 5));
    fx.add_session("alpha", SESSION_UUID, &body(&[claude]));
    fx.add_codex_thread("alpha", THREAD_ID);
    fx
}

fn plan(fx: &Fixture) -> PlanTokens {
    let ids: BTreeSet<String> = ["alpha".to_owned()].into();
    plan_tokens(&fx.work_root, &ids).expect("plan tokens")
}

/// One row per lane, and totals that are the rows' sum.
fn assert_one_row_per_lane(plan: &PlanTokens) {
    let lanes: Vec<&str> = plan.models.iter().map(|row| row.lane.as_str()).collect();
    assert_eq!(lanes.len(), 2, "{:?}", plan.models);
    assert!(lanes.contains(&CLAUDE_LANE), "{:?}", plan.models);
    assert!(lanes.contains(&CODEX_LANE), "{:?}", plan.models);
    let mut sum = TokenUsage::default();
    for row in &plan.models {
        sum.add(&row.tokens);
    }
    assert_eq!(plan.totals, sum);
    assert_eq!(plan.per_stage["alpha"], sum);
    let claude = plan.models.iter().find(|row| row.lane == CLAUDE_LANE);
    assert_eq!(claude.expect("a claude row").tokens, usage(10, 100, 0, 5));
    let codex = plan.models.iter().find(|row| row.lane == CODEX_LANE);
    assert_eq!(codex.expect("a codex row").tokens.output, 7);
}

#[test]
fn a_claude_transcript_and_a_codex_rollout_reach_the_plan_totals_in_their_lanes() {
    let now = unix_now();
    let fx = both_lanes(now);
    fx.add_rollout(THREAD_ID, &rollout(now - 60));
    final_scan(&fx.work_root, Some(&fx.codex_sessions()), now).expect("final scan");
    assert_one_row_per_lane(&plan(&fx));
}

/// A rollout that appears after a pass missed it, as a macOS native
/// worker's does when its boundary is reclaimed, is counted by the final
/// scan however recent the miss.
#[test]
fn the_final_scan_looks_a_recently_missed_thread_up_again() {
    let now = unix_now();
    let fx = both_lanes(now);
    fx.scan(now);
    let missed = plan(&fx);
    assert!(missed.models.iter().all(|row| row.lane == CLAUDE_LANE));
    fx.add_rollout(THREAD_ID, &rollout(now - 30));
    final_scan(&fx.work_root, Some(&fx.codex_sessions()), now + 1).expect("final scan");
    assert_one_row_per_lane(&plan(&fx));
}
