//! Tests for the TOKENS column, the header summary and the `t` key. `flow_data` is
//! the shared builder for a `StatusData` carrying the common.md section 4 flow.

use crossterm::event::{KeyCode, KeyModifiers};

use super::tests::{contains, fixture, make_stage, render_view, screen};
use super::LedgerView;
use crate::commands::status::data::{StageSummary, StatusData};
use crate::commands::status::render::attention_model::attention_entries;
use crate::commands::status::ui::tui::event_handler::{handle_key_event, KeyEventResult};
use crate::commands::status::ui::tui::state::{GraphState, TuiActivityLog};
use crate::models::stage::StageStatus;
use crate::plan::graph::levels;
use crate::quota::{ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind};
use crate::tokens::{
    AgentFlow, AgentKind, ModelUsage, PlanFlow, QuotaLeft, QuotaProvider, StageFlow, TokenFlow,
    TokenUsage,
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

fn plan_total() -> TokenUsage {
    usage(120_000, 1_800_000, 26_000_000, 310_000)
}

fn plan_rate() -> TokenUsage {
    usage(2_000, 36_000, 600_000, 5_200)
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

/// The three section 4 agents: main, subagent and codex.
fn section4_agents() -> Vec<AgentFlow> {
    let mut sub = agent(
        "a04baa183f888b773",
        AgentKind::Subagent,
        "claude-sonnet-5-5",
        [
            usage(50_000, 900_000, 13_000_000, 160_000),
            usage(1_200, 21_000, 350_000, 3_000),
        ],
        12_600,
    );
    sub.agent_type = Some("loom-software-engineer".to_owned());
    vec![
        agent(
            "main-01ed9f4c",
            AgentKind::Main,
            "claude-opus-5-5",
            [
                usage(40_000, 600_000, 9_000_000, 100_000),
                usage(600, 12_000, 200_000, 1_600),
            ],
            7_100,
        ),
        sub,
        agent(
            "codex-01a1396b",
            AgentKind::Codex,
            "gpt-6.1-sol",
            [
                usage(30_000, 300_000, 4_000_000, 50_000),
                usage(200, 3_000, 50_000, 600),
            ],
            1_900,
        ),
    ]
}

fn stage_flow(stage_id: &str) -> StageFlow {
    StageFlow {
        stage_id: stage_id.to_owned(),
        total: plan_total(),
        rate: plan_rate(),
        series: vec![21_600; 120],
        hit_series: Vec::new(),
        agents: section4_agents(),
    }
}

/// The fixture's executing and completed stages carry the section 4 stage flow, and
/// the plan carries the section 4 plan flow with a Claude time-left estimate.
pub(super) fn flow_data() -> StatusData {
    let mut data = fixture();
    data.tokens = Some(TokenFlow {
        observed_at: NOW,
        bucket_secs: 30,
        plan: PlanFlow {
            total: plan_total(),
            rate: plan_rate(),
            series: vec![21_600; 60],
        },
        stages: vec![stage_flow("s-executing"), stage_flow("s-completed")],
        models: Vec::new(),
        quota_left: vec![QuotaLeft {
            provider: QuotaProvider::Claude,
            minutes_left: Some(127),
            lasts_to_reset: false,
        }],
        unreadable_lines: 0,
    });
    data
}

fn row_of<'a>(rows: &'a [String], stage: &str) -> &'a String {
    rows.iter()
        .find(|row| row.contains(stage))
        .unwrap_or_else(|| panic!("no {stage} row in {rows:#?}"))
}

#[test]
fn executing_row_shows_a_sparkline_and_the_rate() {
    let rows = render_view(&flow_data(), 132, 40, false);
    let row = row_of(&rows, "s-executing");
    assert!(row.contains("43.2k"), "{row:?}");
}

#[test]
fn completed_row_shows_the_total_alone() {
    let rows = render_view(&flow_data(), 132, 40, false);
    let row = row_of(&rows, "s-completed");
    assert!(row.contains("2.23M"), "{row:?}");
    assert!(!row.contains("43.2k"), "{row:?}");
}

#[test]
fn rows_without_a_flow_leave_the_cell_empty() {
    let rows = render_view(&flow_data(), 132, 40, false);
    assert!(!row_of(&rows, "s-queued").contains("43.2k"));
    let rows = render_view(&fixture(), 132, 40, false);
    assert!(!contains(&rows, "43.2k"));
    assert!(!contains(&rows, "2.23M"));
}

#[test]
fn stages_that_stopped_before_completing_show_their_total_dimmed() {
    let mut data = flow_data();
    let flow = data.tokens.as_mut().unwrap();
    let stopped = ["s-blocked", "s-failed", "s-conflict", "s-review"];
    flow.stages.extend(stopped.into_iter().map(stage_flow));

    let rows = render_view(&data, 132, 48, false);
    for id in stopped {
        let row = row_of(&rows, id);
        assert!(row.contains("2.23M"), "{row:?}");
        assert!(!row.contains("43.2k"), "{row:?}");
    }
}

/// Render `data` at 132 columns with the tokens pane toggled on.
fn render_with_pane(data: &StatusData, height: u16) -> Vec<String> {
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
    screen(132, height, &view)
}

#[test]
fn the_pane_says_so_when_there_is_no_flow_or_no_active_agent() {
    let rows = render_with_pane(&fixture(), 40);
    assert!(contains(&rows, "fresh per 30 s cell"), "{rows:#?}");
    assert!(contains(&rows, "no token data yet"), "{rows:#?}");

    let mut data = flow_data();
    for stage in &mut data.tokens.as_mut().unwrap().stages {
        stage
            .agents
            .iter_mut()
            .for_each(|agent| agent.active = false);
    }
    let rows = render_with_pane(&data, 40);
    assert!(
        row_of(&rows, "fresh per 30 s cell").contains("no active agent"),
        "{rows:#?}"
    );
    assert!(!contains(&rows, "no token data yet"), "{rows:#?}");
}

#[test]
fn header_summary_needs_room_beside_the_version_label() {
    let wide = render_view(&flow_data(), 150, 40, false);
    assert!(contains(&wide, "tokens 43.2k/m"));
    assert!(contains(&wide, "2.23M total · hit 93.1%"));
    assert!(contains(&wide, crate::version::LABEL));

    let narrow = render_view(&flow_data(), 100, 40, false);
    assert!(!contains(&narrow, "tokens 43.2k/m"));
    assert!(contains(&narrow, crate::version::LABEL));
}

#[test]
fn t_toggles_the_tokens_pane() {
    let mut graph = GraphState::default();
    let result = handle_key_event(KeyCode::Char('t'), KeyModifiers::NONE, &mut graph);
    assert!(matches!(result, KeyEventResult::ToggleTokens));
}

#[test]
fn footer_hints_offer_the_tokens_key() {
    let rows = render_view(&fixture(), 132, 40, false);
    assert!(rows.last().unwrap().ends_with("t tokens · q quit"));
}

/// One active stage with a main agent and a subagent (four live rows) and three
/// models with one effort each (a seven-row by-model block).
fn live_and_models_flow() -> TokenFlow {
    let mut flow = flow_data().tokens.unwrap();
    let mut stage = stage_flow("s-executing");
    stage.agents.truncate(2);
    flow.stages = vec![stage];
    flow.models = [
        ("claude-sonnet-5-5", "medium"),
        ("claude-opus-5-5", "xhigh"),
        ("gpt-6.1-sol", "xhigh"),
    ]
    .into_iter()
    .map(|(model, effort)| ModelUsage {
        model: model.to_owned(),
        effort: Some(effort.to_owned()),
        total: usage(50_000, 900_000, 13_000_000, 160_000),
    })
    .collect();
    flow
}

/// Render the pane over one executing stage, one activity entry and a Claude quota
/// line, so the footer takes two rows.
fn render_pane_scenario(width: u16, height: u16) -> Vec<String> {
    let data = StatusData {
        stages: vec![make_stage("s-executing", StageStatus::Executing)],
        tokens: Some(live_and_models_flow()),
        quota: QuotaSnapshot {
            claude: Some(ProviderQuota {
                observed_at: NOW,
                windows: vec![QuotaWindow {
                    kind: WindowKind::FiveHour,
                    used_percent: 48.0,
                    resets_at: Some(NOW + 7_200),
                }],
                plan: None,
                error: None,
            }),
            codex: None,
        },
        ..StatusData::default()
    };
    let levels = levels::compute_all_levels(
        &data.stages,
        |stage| stage.id.as_str(),
        |stage| &stage.dependencies,
    );
    let ordered: Vec<&StageSummary> = data.stages.iter().collect();
    let mut activity = TuiActivityLog::new();
    activity.update(&ordered);
    let view = LedgerView {
        data: &data,
        levels: &levels,
        ordered: &ordered,
        attention: &[],
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

#[test]
fn a_short_terminal_keeps_the_live_rows_when_the_model_block_does_not_fit() {
    // The whole pane is twelve rows, which leaves the table none at 24 rows; the four
    // live rows alone leave it eight.
    let rows = render_pane_scenario(100, 24);

    assert!(contains(&rows, "fresh per 30 s cell"), "{rows:#?}");
    assert!(
        rows.iter()
            .any(|row| row.contains("main") && row.contains("live")),
        "{rows:#?}"
    );
    assert!(!contains(&rows, "BY MODEL"), "{rows:#?}");
    assert!(!contains(&rows, "gpt-6.1-sol"), "{rows:#?}");
}

#[test]
fn a_tall_terminal_shows_the_live_rows_and_the_model_block() {
    let rows = render_pane_scenario(100, 40);

    assert!(contains(&rows, "fresh per 30 s cell"), "{rows:#?}");
    assert!(contains(&rows, "BY MODEL"), "{rows:#?}");
    assert!(contains(&rows, "gpt-6.1-sol"), "{rows:#?}");
}
