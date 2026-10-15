//! Frozen contract for the `t` pane's by-model block: plan-wide token totals per
//! model, its share of fresh tokens, and the efforts under it.

use super::super::tests::{contains, fixture, screen};
use super::super::LedgerView;
use crate::commands::status::data::{StageSummary, StatusData};
use crate::commands::status::render::attention_model::attention_entries;
use crate::commands::status::ui::tui::state::TuiActivityLog;
use crate::plan::graph::levels;
use crate::tokens::{AgentFlow, AgentKind, ModelUsage, PlanFlow, StageFlow, TokenFlow, TokenUsage};

const NOW: i64 = 1_788_523_200;

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
        model: model.to_owned(),
        effort: Some(effort.to_owned()),
        total,
    }
}

/// The four entries of common.md section 5, in their wire order.
fn models() -> Vec<ModelUsage> {
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

fn flow() -> TokenFlow {
    let total = usage(120_000, 1_800_000, 26_000_000, 310_000);
    let rate = usage(2_000, 36_000, 600_000, 5_200);
    let main = AgentFlow {
        id: "main-01ed9f4c".to_owned(),
        kind: AgentKind::Main,
        agent_type: None,
        model: Some("claude-opus-5-5".to_owned()),
        active: true,
        total,
        rate,
        series: vec![21_600; 120],
    };
    TokenFlow {
        observed_at: NOW,
        bucket_secs: 30,
        plan: PlanFlow {
            total,
            rate,
            series: vec![21_600; 60],
        },
        stages: vec![StageFlow {
            stage_id: "s-executing".to_owned(),
            total,
            rate,
            series: vec![21_600; 120],
            hit_series: vec![Some(94); 120],
            agents: vec![main],
        }],
        models: models(),
        quota_left: Vec::new(),
        unreadable_lines: 0,
    }
}

fn rows(data: &StatusData, width: u16, height: u16) -> Vec<String> {
    let levels = levels::compute_all_levels(
        &data.stages,
        |stage| stage.id.as_str(),
        |stage| &stage.dependencies,
    );
    let ordered: Vec<&StageSummary> = data.stages.iter().collect();
    let attention = attention_entries(&data.stages);
    let activity = TuiActivityLog::new();
    let view = LedgerView {
        data,
        levels: &levels,
        ordered: &ordered,
        attention: &attention,
        activity: &activity,
        alerts: &[],
        spinner: '⠋',
        scroll_y: 0,
        legend_open: false,
        tick_age_secs: Some(2),
        last_error: None,
        now_epoch: NOW,
        scrollable: false,
        show_tokens: true,
    };
    screen(width, height, &view)
}

fn has_row(rows: &[String], needles: &[&str]) -> bool {
    rows.iter()
        .any(|row| needles.iter().all(|needle| row.contains(needle)))
}

#[test]
fn pane_shows_model_breakdown() {
    let mut data = fixture();
    data.tokens = Some(flow());
    let shown = rows(&data, 132, 60);

    assert!(
        has_row(&shown, &["claude-sonnet-5-5", "50%", "1.11M"]),
        "no sonnet row with its 50% share and 1.11M fresh: {shown:#?}"
    );
    assert!(
        has_row(&shown, &["claude-opus-5-5", "33%"]),
        "no opus row whose share sums its two efforts (33%): {shown:#?}"
    );
    assert!(contains(&shown, "xhigh"), "no xhigh effort row: {shown:#?}");
    assert!(
        shown
            .iter()
            .any(|row| row.contains("high") && !row.contains("xhigh")),
        "no high effort row: {shown:#?}"
    );
}
