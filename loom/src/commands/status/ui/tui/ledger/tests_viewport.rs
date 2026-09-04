//! Coverage for viewport/scroll bookkeeping and panel-presence branches that
//! `tests.rs`'s `screen()` helper (which discards `RenderOutcome`) and its
//! all-defaults `render_view()` never exercise.

use ratatui::{backend::TestBackend, Terminal};

use super::tests::{contains, fixture, make_stage, render_view, screen};
use super::{render, LedgerView, RenderOutcome};
use crate::commands::status::data::{MergeSummary, ProgressSummary, StageSummary, StatusData};
use crate::commands::status::render::attention_model::attention_entries;
use crate::commands::status::ui::tui::state::TuiActivityLog;
use crate::models::stage::StageStatus;
use crate::plan::graph::levels;

/// Like `tests::render_view`, but exposes the `scroll_y` and `last_error`
/// fields every case in `tests.rs` pins to their defaults.
fn render_view_with(
    data: &StatusData,
    width: u16,
    height: u16,
    scroll_y: u16,
    last_error: Option<&str>,
) -> Vec<String> {
    let levels = levels::compute_all_levels(
        &data.stages,
        |stage| stage.id.as_str(),
        |stage| &stage.dependencies,
    );
    let mut ordered: Vec<&StageSummary> = data.stages.iter().collect();
    ordered.sort_by(|left, right| {
        let left_level = levels.get(&left.id).copied().unwrap_or_default();
        let right_level = levels.get(&right.id).copied().unwrap_or_default();
        left_level
            .cmp(&right_level)
            .then_with(|| left.id.cmp(&right.id))
    });
    let attention = attention_entries(&data.stages);
    let activity = TuiActivityLog::new();
    let alerts = Vec::new();
    let view = LedgerView {
        data,
        levels: &levels,
        ordered: &ordered,
        attention: &attention,
        activity: &activity,
        alerts: &alerts,
        spinner: '⠋',
        scroll_y,
        legend_open: false,
        tick_age_secs: Some(2),
        last_error,
    };
    screen(width, height, &view)
}

fn many_stages(count: usize) -> Vec<StageSummary> {
    (0..count)
        .map(|i| make_stage(&format!("stage-{i:02}"), StageStatus::Completed))
        .collect()
}

fn data_without_attention() -> StatusData {
    StatusData {
        stages: vec![make_stage("s-completed", StageStatus::Completed)],
        merge: MergeSummary {
            merged: vec!["s-completed".to_owned()],
            pending: Vec::new(),
            conflicts: Vec::new(),
        },
        progress: ProgressSummary {
            total: 1,
            completed: 1,
            ..ProgressSummary::default()
        },
        plan_name: Some("no-attention".to_owned()),
    }
}

#[test]
fn table_viewport_rows_matches_the_computed_table_budget() {
    let data = StatusData::default();
    let view = LedgerView {
        data: &data,
        levels: &Default::default(),
        ordered: &[],
        attention: &[],
        activity: &TuiActivityLog::new(),
        alerts: &[],
        spinner: '⠋',
        scroll_y: 0,
        legend_open: false,
        tick_age_secs: None,
        last_error: None,
    };
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut outcome = RenderOutcome::default();
    terminal
        .draw(|frame| outcome = render(frame, &view))
        .unwrap();

    // Zero alerts/attention/activity: reserved rows are just the header (4),
    // the gap under it (1), the table's own top gap (1), and the footer (1),
    // so budget.table = 40 - 7 = 33, and table_viewport_rows = 33 - 2.
    assert_eq!(outcome.table_viewport_rows, 31);
}

#[test]
fn scroll_y_hides_skipped_stages_and_shows_the_next_one() {
    let stages = many_stages(20);
    let data = StatusData {
        progress: ProgressSummary {
            total: stages.len(),
            completed: stages.len(),
            ..ProgressSummary::default()
        },
        stages,
        ..StatusData::default()
    };

    let rows = render_view_with(&data, 120, 20, 5, None);

    for i in 0..5 {
        assert!(
            !contains(&rows, &format!("stage-{i:02}")),
            "stage-{i:02} should be scrolled out of view"
        );
    }
    assert!(
        contains(&rows, "stage-05"),
        "the stage right after the scrolled-out range should be visible"
    );
}

#[test]
fn attention_panel_is_absent_when_no_stage_needs_attention() {
    let data = data_without_attention();
    let rows = render_view(&data, 120, 40, false);
    assert!(!contains(&rows, "NEEDS ATTENTION"));
}

#[test]
fn legend_closed_hides_the_legend_overlay() {
    let data = fixture();
    let rows = render_view(&data, 120, 40, false);
    assert!(!contains(&rows, "Stage states"));
}

#[test]
fn footer_shows_the_last_error_when_present() {
    let data = fixture();
    let rows = render_view_with(&data, 120, 40, 0, Some("daemon exploded"));
    assert!(contains(&rows, "Error: daemon exploded"));
}
