//! Tests for the TOKENS column, the header summary and the `t` key. `flow_data` is
//! the shared builder for a `StatusData` carrying the common.md section 4 flow.

use crossterm::event::{KeyCode, KeyModifiers};

use super::tests::{contains, fixture, render_view};
use crate::commands::status::data::StatusData;
use crate::commands::status::ui::tui::event_handler::{handle_key_event, KeyEventResult};
use crate::commands::status::ui::tui::state::GraphState;
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
