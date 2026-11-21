use super::{
    aggregate, plan_tokens, AgentTotals, ModelUsage, StageTotals, CLAUDE_LANE, CODEX_LANE,
};
use crate::tokens::accumulate::MESSAGE_TABLE_CAP;
use crate::tokens::model::{self, TokenUsage, OTHER_MODEL, UNKNOWN_MODEL};
use crate::tokens::tests_fixture::{
    append, body, bulky_body, claude_line, unix_now, usage, Fixture, SESSION_UUID,
};
use crate::tokens::tracker::{final_scan, CHECKPOINT_FILE};
use std::collections::BTreeSet;
use std::fs;

const OPUS: &str = "claude-opus-5-5";

fn entry(model: &str, effort: Option<&str>, total: TokenUsage) -> model::ModelUsage {
    model::ModelUsage {
        model: model.to_owned(),
        effort: effort.map(str::to_owned),
        total,
    }
}

fn agent(lane: &'static str, by_model: Vec<model::ModelUsage>) -> AgentTotals {
    let mut total = TokenUsage::default();
    for entry in &by_model {
        total.add(&entry.total);
    }
    AgentTotals {
        lane,
        total,
        by_model,
    }
}

fn ids(ids: &[&str]) -> BTreeSet<String> {
    ids.iter().map(|id| (*id).to_owned()).collect()
}

fn row(
    model: &str,
    effort: Option<&str>,
    lane: &str,
    agents: u32,
    tokens: TokenUsage,
) -> ModelUsage {
    ModelUsage {
        model: model.to_owned(),
        effort: effort.map(str::to_owned),
        lane: lane.to_owned(),
        agents,
        tokens,
    }
}

#[test]
fn each_agent_counts_once_per_model_effort_and_lane_it_used() {
    let mut stages = StageTotals::new();
    let main = agent(
        CLAUDE_LANE,
        vec![
            entry(OPUS, Some("high"), usage(10, 0, 100, 5)),
            entry(UNKNOWN_MODEL, None, usage(1, 0, 0, 0)),
            // Retracted to zero: the agent used nothing there.
            entry("claude-haiku", None, TokenUsage::default()),
        ],
    );
    let sub = agent(
        CLAUDE_LANE,
        vec![entry(OPUS, Some("high"), usage(2, 0, 0, 0))],
    );
    let codex = agent(
        CODEX_LANE,
        vec![
            entry(OPUS, Some("high"), usage(0, 0, 0, 1)),
            entry(OTHER_MODEL, None, usage(0, 0, 0, 3)),
        ],
    );
    stages.insert("alpha".into(), vec![main, sub, codex]);
    stages.insert(
        "zeta".into(),
        vec![agent(
            CLAUDE_LANE,
            vec![entry(OPUS, None, usage(1000, 0, 0, 0))],
        )],
    );

    let plan = aggregate(&stages, &ids(&["alpha", "beta"]));

    assert_eq!(plan.totals, usage(13, 0, 100, 9));
    assert_eq!(plan.per_stage.len(), 2, "zeta is another plan's stage");
    assert_eq!(plan.per_stage["alpha"], plan.totals);
    assert_eq!(plan.per_stage["beta"], TokenUsage::default());
    assert_eq!(
        plan.models,
        vec![
            row(OPUS, Some("high"), CLAUDE_LANE, 2, usage(12, 0, 100, 5)),
            row(OTHER_MODEL, None, CODEX_LANE, 1, usage(0, 0, 0, 3)),
            row(OPUS, Some("high"), CODEX_LANE, 1, usage(0, 0, 0, 1)),
            row(UNKNOWN_MODEL, None, CLAUDE_LANE, 1, usage(1, 0, 0, 0)),
        ]
    );
}

#[test]
fn ties_sort_by_model_then_effort_with_none_last() {
    let mut stages = StageTotals::new();
    let one = usage(1, 0, 0, 0);
    stages.insert(
        "alpha".into(),
        vec![agent(
            CLAUDE_LANE,
            vec![
                entry("b", None, one),
                entry("b", Some("low"), one),
                entry("a", None, one),
            ],
        )],
    );
    let plan = aggregate(&stages, &ids(&["alpha"]));
    let keys: Vec<(&str, Option<&str>)> = plan
        .models
        .iter()
        .map(|row| (row.model.as_str(), row.effort.as_deref()))
        .collect();
    assert_eq!(keys, vec![("a", None), ("b", Some("low")), ("b", None)]);
}

#[test]
fn totals_saturate() {
    let huge = usage(u64::MAX, 0, u64::MAX, 0);
    let mut stages = StageTotals::new();
    stages.insert(
        "alpha".into(),
        vec![
            agent(CLAUDE_LANE, vec![entry(OPUS, None, huge)]),
            agent(CLAUDE_LANE, vec![entry(OPUS, None, huge)]),
        ],
    );
    let plan = aggregate(&stages, &ids(&["alpha"]));
    assert_eq!(plan.totals, huge);
    assert_eq!(plan.models[0].tokens, huge);
    assert_eq!(plan.models[0].agents, 2);
}

fn line(msg_id: &str, ts: i64, cache_write: u64) -> String {
    claude_line(msg_id, ts, OPUS, &usage(0, cache_write, 0, 0))
}

fn alpha(fx: &Fixture) -> TokenUsage {
    plan_tokens(&fx.work_root, &ids(&["alpha"]))
        .expect("plan tokens")
        .per_stage["alpha"]
}

#[test]
fn the_final_scan_counts_a_last_line_without_its_newline_once() {
    let fx = Fixture::new();
    let now = unix_now();
    let text = format!(
        "{}{}",
        body(&[line("m1", now - 90, 100)]),
        line("m2", now - 60, 7)
    );
    fx.add_session("alpha", "a1b2c3d4-0000-4000-8000-000000000001", &text);
    assert_eq!(
        fx.scan(now).plan.total.cache_write,
        100,
        "a tracker pass waits"
    );

    final_scan(&fx.work_root, Some(&fx.codex_sessions()), now).expect("final scan");
    assert_eq!(alpha(&fx).cache_write, 107);
    assert_eq!(fx.scan(now).plan.total.cache_write, 107, "counted once");

    // In memory: a checkpoint that could not be written loses nothing.
    fs::remove_file(fx.tokens_dir().join(CHECKPOINT_FILE)).expect("remove checkpoint");
    assert_eq!(alpha(&fx).cache_write, 107);
}

#[test]
fn the_final_scan_leaves_a_half_written_line_uncounted() {
    let fx = Fixture::new();
    let now = unix_now();
    let second = line("m2", now - 60, 7);
    let (head, rest) = second.split_at(second.len() / 2);
    let text = format!("{}{head}", body(&[line("m1", now - 90, 100)]));
    let transcript = fx.add_session("alpha", "a1b2c3d4-0000-4000-8000-000000000002", &text);

    final_scan(&fx.work_root, Some(&fx.codex_sessions()), now).expect("final scan");
    assert_eq!(alpha(&fx).cache_write, 100);

    append(&transcript, &format!("{rest}\n"));
    assert_eq!(fx.scan(now).plan.total.cache_write, 107, "read whole, once");
}

#[test]
fn without_a_final_scan_the_checkpoint_is_read() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session(
        "alpha",
        "a1b2c3d4-0000-4000-8000-000000000003",
        &body(&[line("m1", now - 90, 100)]),
    );
    let wanted = ids(&["alpha", "beta"]);
    let before = plan_tokens(&fx.work_root, &wanted).expect("no tokens directory yet");
    assert_eq!(before.totals, TokenUsage::default());
    assert_eq!(before.per_stage.len(), 2);

    fx.scan(now);
    let plan = plan_tokens(&fx.work_root, &wanted).expect("plan tokens");
    assert_eq!(plan.totals, usage(0, 100, 0, 0));
    assert_eq!(
        plan.models,
        vec![row(OPUS, None, CLAUDE_LANE, 1, usage(0, 100, 0, 0))]
    );

    fs::write(fx.tokens_dir().join(CHECKPOINT_FILE), "{").expect("corrupt");
    let error = plan_tokens(&fx.work_root, &wanted).expect_err("unparseable checkpoint");
    assert!(format!("{error:#}").contains("unparseable"), "{error:#}");
}

#[test]
fn a_symlinked_tokens_directory_is_refused() {
    let fx = Fixture::new();
    let outside = fx.tmp.path().join("outside");
    fs::create_dir(&outside).expect("outside");
    std::os::unix::fs::symlink(&outside, fx.work_root.join("tokens")).expect("link");
    assert!(plan_tokens(&fx.work_root, &ids(&["alpha"])).is_err());
}

#[test]
fn the_final_scan_keeps_totals_a_checkpoint_too_big_to_write_would_lose() {
    let fx = Fixture::new();
    let now = unix_now();
    fx.add_session("alpha", SESSION_UUID, &bulky_body("main", now));
    fx.add_subagent(SESSION_UUID, "agent-a1.jsonl", &bulky_body("sub", now));

    final_scan(&fx.work_root, Some(&fx.codex_sessions()), now).expect("final scan");

    assert!(
        !fx.tokens_dir().join(CHECKPOINT_FILE).exists(),
        "over the cap"
    );
    let plan = plan_tokens(&fx.work_root, &ids(&["alpha"])).expect("plan tokens");
    assert_eq!(plan.totals.cache_write, 2 * MESSAGE_TABLE_CAP as u64);
    assert_eq!(plan.models[0].agents, 2, "main and subagent");
}

#[path = "plan_totals_lanes_tests.rs"]
mod lanes;
