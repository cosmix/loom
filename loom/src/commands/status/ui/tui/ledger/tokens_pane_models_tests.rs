//! The by-model block inside the t pane.

use super::*;
use crate::tokens::{ModelUsage, PlanFlow, TokenUsage};

fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

fn entry(model: &str, effort: &str, total: TokenUsage) -> ModelUsage {
    ModelUsage {
        model: model.into(),
        effort: Some(effort.into()),
        total,
    }
}

fn four_models() -> Vec<ModelUsage> {
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

fn flow(active: bool, models: Vec<ModelUsage>) -> TokenFlow {
    let total = usage(120_000, 1_800_000, 26_000_000, 310_000);
    let rate = usage(2_000, 36_000, 600_000, 5_200);
    let main = AgentFlow {
        id: "main-01ed9f4c".into(),
        kind: AgentKind::Main,
        agent_type: None,
        model: Some("claude-opus-5-5".into()),
        active,
        total,
        rate,
        series: vec![7_100; 120],
    };
    TokenFlow {
        observed_at: 1_788_523_200,
        bucket_secs: 30,
        plan: PlanFlow {
            total,
            rate,
            series: vec![21_600; 60],
        },
        stages: vec![StageFlow {
            stage_id: "server".into(),
            total,
            rate,
            series: vec![21_600; 120],
            hit_series: Vec::new(),
            agents: vec![main],
        }],
        models,
        quota_left: Vec::new(),
        unreadable_lines: 0,
    }
}

fn texts(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

fn position(rows: &[String], needle: &str) -> usize {
    rows.iter()
        .position(|row| row.contains(needle))
        .unwrap_or_else(|| panic!("no row with {needle}: {rows:#?}"))
}

#[test]
fn test_block_follows_the_live_rows_after_a_blank_line() {
    let rows = texts(&tokens_pane_lines(
        &flow(true, four_models()),
        132,
        PANE_MAX_ROWS,
    ));
    let live = texts(&tokens_pane_lines(
        &flow(true, Vec::new()),
        132,
        PANE_MAX_ROWS,
    ));
    let title = position(&rows, "BY MODEL");
    assert_eq!(title, live.len() + 1);
    assert!(rows[title - 1].trim().is_empty());
    assert!(position(&rows, "claude-sonnet-5-5") > title);
}

#[test]
fn test_block_shows_without_an_active_agent() {
    let rows = texts(&tokens_pane_lines(
        &flow(false, four_models()),
        132,
        PANE_MAX_ROWS,
    ));
    assert!(rows[0].contains("no active agent"));
    assert!(position(&rows, "BY MODEL") > 0);
    assert!(position(&rows, "claude-sonnet-5-5") > 0);
}

#[test]
fn test_empty_models_leave_the_pane_unchanged() {
    let rows = tokens_pane_lines(&flow(true, Vec::new()), 132, PANE_MAX_ROWS);
    assert_eq!(rows.len(), 3);
    assert!(!texts(&rows).iter().any(|row| row.contains("BY MODEL")));
}

#[test]
fn test_small_max_rows_cut_from_the_bottom() {
    let full = tokens_pane_lines(&flow(true, four_models()), 132, PANE_MAX_ROWS);
    let cut = tokens_pane_lines(&flow(true, four_models()), 132, 6);
    assert_eq!(cut.len(), 6);
    assert_eq!(texts(&cut), texts(&full)[..6]);
}
