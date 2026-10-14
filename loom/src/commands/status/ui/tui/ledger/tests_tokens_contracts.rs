//! Frozen contracts for the ledger's token flow: the TOKENS column, the header summary,
//! the per-agent pane, and the quota line's time-left suffix.

use super::super::quota::quota_line;
use super::super::tests::{contains, fixture, screen};
use super::super::LedgerView;
use crate::commands::status::data::{StageSummary, StatusData};
use crate::commands::status::render::attention_model::attention_entries;
use crate::commands::status::ui::tui::state::TuiActivityLog;
use crate::plan::graph::levels;
use crate::quota::{ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind};
use crate::tokens::{
    AgentFlow, AgentKind, PlanFlow, QuotaLeft, QuotaProvider, StageFlow, TokenFlow, TokenUsage,
};

const NOW: i64 = 1_788_523_200;

fn usage(input: u64, cache_write: u64, cache_read: u64, output: u64) -> TokenUsage {
    TokenUsage {
        input,
        cache_write,
        cache_read,
        output,
    }
}

fn agent(id: &str, kind: AgentKind, model: &str, usages: [TokenUsage; 2], per: u64) -> AgentFlow {
    AgentFlow {
        id: id.to_owned(),
        kind,
        agent_type: None,
        model: Some(model.to_owned()),
        active: true,
        total: usages[0],
        rate: usages[1],
        series: vec![per; 120],
    }
}

fn agents() -> Vec<AgentFlow> {
    let main_usage = [
        usage(40_000, 600_000, 9_000_000, 100_000),
        usage(600, 12_000, 200_000, 1_600),
    ];
    let sub_usage = [
        usage(50_000, 900_000, 13_000_000, 160_000),
        usage(1_200, 21_000, 350_000, 3_000),
    ];
    let codex_usage = [
        usage(30_000, 300_000, 4_000_000, 50_000),
        usage(200, 3_000, 50_000, 600),
    ];
    let mut sub = agent(
        "a04baa183f888b773",
        AgentKind::Subagent,
        "claude-sonnet-5-5",
        sub_usage,
        12_600,
    );
    sub.agent_type = Some("loom-software-engineer".to_owned());
    vec![
        agent(
            "main-01ed9f4c",
            AgentKind::Main,
            "claude-opus-5-5",
            main_usage,
            7_100,
        ),
        sub,
        agent(
            "codex-01a1396b",
            AgentKind::Codex,
            "gpt-6.1-sol",
            codex_usage,
            1_900,
        ),
    ]
}

/// The common.md section 4 flow, on `fixture()`'s executing stage.
fn flow() -> TokenFlow {
    let total = usage(120_000, 1_800_000, 26_000_000, 310_000);
    let rate = usage(2_000, 36_000, 600_000, 5_200);
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
            agents: agents(),
        }],
        quota_left: vec![QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left: Some(127),
            lasts_to_reset: false,
        }],
        unreadable_lines: 0,
    }
}

fn with_flow() -> StatusData {
    let mut data = fixture();
    data.tokens = Some(flow());
    data
}

fn rows(data: &StatusData, width: u16, height: u16, show_tokens: bool) -> Vec<String> {
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
        show_tokens,
    };
    screen(width, height, &view)
}

fn header_row(rows: &[String]) -> &String {
    rows.iter()
        .find(|row| row.contains("CONTEXT"))
        .unwrap_or_else(|| panic!("no header row with CONTEXT in {rows:#?}"))
}

fn window(kind: WindowKind, used_percent: f64, resets_at: i64) -> QuotaWindow {
    QuotaWindow {
        kind,
        used_percent,
        resets_at: Some(resets_at),
    }
}

/// The snapshot pinned in the stage's contract surface (quota_tests.rs's is private).
fn pinned_snapshot() -> QuotaSnapshot {
    QuotaSnapshot {
        claude: Some(ProviderQuota {
            observed_at: NOW,
            windows: vec![
                window(WindowKind::FiveHour, 48.0, 1_788_531_180),
                window(WindowKind::SevenDay, 31.0, 1_788_876_000),
            ],
            plan: None,
            error: None,
        }),
        codex: Some(ProviderQuota {
            observed_at: 1_788_522_960,
            windows: vec![window(WindowKind::SevenDay, 63.0, 1_788_728_400)],
            plan: Some("pro".to_owned()),
            error: None,
        }),
    }
}

#[test]
fn tokens_column_between_context_and_time() {
    let rows = rows(&with_flow(), 132, 40, false);
    let header = header_row(&rows);
    let position = |label: &str| {
        header
            .find(label)
            .unwrap_or_else(|| panic!("missing {label} in header {header:?}"))
    };
    let context = position("CONTEXT");
    let tokens = position("TOKENS");
    let time = position("TIME");
    let merge = position("MERGE");
    assert!(
        context < tokens && tokens < time && time < merge,
        "expected CONTEXT, TOKENS, TIME, MERGE in order: {header:?}"
    );
}

#[test]
fn tokens_column_drop_order() {
    let data = with_flow();
    let at_90 = rows(&data, 90, 40, false);
    let header = header_row(&at_90);
    assert!(
        header.contains("TOKENS"),
        "TOKENS dropped at 90: {header:?}"
    );
    assert!(
        !header.contains("DEPENDS ON"),
        "DEPENDS ON kept at 90: {header:?}"
    );

    let at_80 = rows(&data, 80, 40, false);
    let header = header_row(&at_80);
    assert!(!header.contains("TOKENS"), "TOKENS kept at 80: {header:?}");
}

#[test]
fn token_pane_shows_agents_when_toggled() {
    let data = with_flow();
    let starts_with_main =
        |rows: &[String]| rows.iter().any(|row| row.trim_start().starts_with("main"));

    let shown = rows(&data, 132, 48, true);
    assert!(
        contains(&shown, "fresh per 30 s cell"),
        "pane missing with show_tokens: {shown:#?}"
    );
    assert!(starts_with_main(&shown), "no main agent row: {shown:#?}");

    let hidden = rows(&data, 132, 48, false);
    assert!(
        !contains(&hidden, "fresh per 30 s cell"),
        "pane shown without show_tokens: {hidden:#?}"
    );
    assert!(
        !starts_with_main(&hidden),
        "main agent row shown: {hidden:#?}"
    );
}

#[test]
fn tokens_cell_shows_stage_rate() {
    let shown = rows(&with_flow(), 132, 40, false);
    assert!(
        shown
            .iter()
            .any(|row| row.contains("s-executing") && row.contains("43.2k")),
        "no s-executing row with the 43.2k rate: {shown:#?}"
    );

    let hidden = rows(&fixture(), 132, 40, false);
    assert!(
        !contains(&hidden, "43.2k"),
        "rate shown without a flow: {hidden:#?}"
    );
}

#[test]
fn footer_quota_shows_time_left() {
    let mut data = with_flow();
    data.quota = pinned_snapshot();
    let rows = rows(&data, 132, 40, false);
    let quota = rows
        .iter()
        .find(|row| row.contains("claude 5h"))
        .unwrap_or_else(|| panic!("no quota row: {rows:#?}"));
    assert!(
        quota.contains("~2h07m left"),
        "footer quota lacks time left: {quota:?}"
    );
}

#[test]
fn header_shows_plan_token_summary() {
    let shown = rows(&with_flow(), 150, 40, false);
    assert!(
        shown.iter().any(|row| row.contains("tokens 43.2k/m")
            && row.contains("2.23M total")
            && row.contains("hit 93.1%")),
        "no header summary row: {shown:#?}"
    );

    let hidden = rows(&fixture(), 150, 40, false);
    assert!(
        !contains(&hidden, "tokens 43.2k/m"),
        "summary shown without a flow: {hidden:#?}"
    );
}

#[test]
fn quota_line_shows_time_left() {
    let snapshot = QuotaSnapshot {
        claude: Some(ProviderQuota {
            observed_at: NOW,
            windows: vec![window(WindowKind::FiveHour, 62.0, 1_788_531_180)],
            plan: None,
            error: None,
        }),
        codex: None,
    };
    let estimate = |minutes_left, lasts_to_reset| {
        let left = [QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left,
            lasts_to_reset,
        }];
        quota_line(&snapshot, &left, NOW, 120).to_string()
    };

    let line = estimate(Some(127), false);
    assert!(line.contains("~2h07m left"), "no time left: {line:?}");
    let line = estimate(None, true);
    assert!(
        line.contains("lasts to reset"),
        "no lasts to reset: {line:?}"
    );
}

#[test]
fn quota_tiers_unchanged_at_120() {
    let line = quota_line(&pinned_snapshot(), &[], NOW, 120).to_string();
    assert_eq!(
        line,
        " claude 5h ━━━━╌╌╌╌ 48% · 2h13m  7d ━━╌╌╌╌╌╌ 31% · 4d2h │ codex 5h —  7d ━━━━━╌╌╌ 63% · 2d9h"
    );
}
