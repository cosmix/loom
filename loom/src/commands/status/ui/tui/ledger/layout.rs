//! Vertical layout and rendering for the live ledger dashboard.

use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::layout_tokens::{areas, budget, Budget};
use super::tokens_pane::{no_data_lines, tokens_pane_lines, PANE_MAX_ROWS};
use super::{
    columns, header, legend, panels, quota, rows, LedgerView, RenderOutcome, MIN_COLS, MIN_ROWS,
};
use crate::commands::status::ui::theme::Theme;
use crate::commands::status::ui::tui::renderer;
use crate::models::stage::StageStatus;

/// Render the responsive live ledger dashboard.
pub fn render(frame: &mut Frame, view: &LedgerView) -> RenderOutcome {
    let area = frame.area();
    if area.width < MIN_COLS || area.height < MIN_ROWS {
        render_size_notice(frame, area);
        return RenderOutcome::default();
    }

    let budget = render_dashboard(frame, area, view);
    if view.legend_open {
        legend::render_legend_overlay(frame, area);
    }
    RenderOutcome {
        table_viewport_rows: budget.table.saturating_sub(2),
    }
}

/// The tokens pane's lines while it is toggled on, else none; without a flow it is
/// the title and a note saying so.
fn pane_lines(view: &LedgerView, width: u16) -> Vec<Line<'static>> {
    if !view.show_tokens {
        return Vec::new();
    }
    match view.data.tokens.as_ref() {
        Some(flow) => tokens_pane_lines(flow, width, PANE_MAX_ROWS),
        None => no_data_lines(width, PANE_MAX_ROWS),
    }
}

fn render_dashboard(frame: &mut Frame, area: Rect, view: &LedgerView) -> Budget {
    let mut pane = pane_lines(view, area.width);
    let budget = budget(
        area.height,
        view.alerts.len(),
        panels::attention_line_count(view.attention, area.width),
        view.activity.len(),
        quota::has_quota(&view.data.quota),
        u16::try_from(pane.len()).unwrap_or(PANE_MAX_ROWS),
    );
    let areas = areas(area, budget);
    header::render_header(frame, areas.header, view);
    renderer::render_scheduler_alerts(frame, areas.alerts, view.alerts);
    render_table(frame, areas.table, view);
    pane.truncate(usize::from(areas.tokens.height));
    frame.render_widget(Paragraph::new(pane), areas.tokens);
    panels::render_attention(frame, areas.attention, view.attention);
    panels::render_activity(frame, areas.activity, view.activity);

    let present = present_statuses(view);
    panels::render_footer(frame, areas.footer, &present, view);
    budget
}

fn render_table(frame: &mut Frame, area: Rect, view: &LedgerView) {
    let columns = columns::columns_for_width(area.width);
    let mut lines = vec![
        columns::header_line(&columns),
        columns::rule_line(area.width),
    ];
    lines.extend(
        view.ordered
            .iter()
            .skip(view.scroll_y as usize)
            .map(|stage| {
                let level = view.levels.get(&stage.id).copied().unwrap_or_default();
                let flow = view.data.tokens.as_ref().and_then(|flow| {
                    flow.stages
                        .iter()
                        .find(|candidate| candidate.stage_id == stage.id)
                });
                rows::stage_row(stage, level, &columns, view.ordered, flow)
            }),
    );
    frame.render_widget(Paragraph::new(lines), area);
}

fn present_statuses(view: &LedgerView) -> Vec<StageStatus> {
    view.ordered.iter().fold(Vec::new(), |mut present, stage| {
        if !present.contains(&stage.status) {
            present.push(stage.status.clone());
        }
        present
    })
}

fn render_size_notice(frame: &mut Frame, area: Rect) {
    let message = format!(
        "loom status --live needs a terminal of at least 64×16; this one is {}×{}",
        area.width, area.height
    );
    let notice = Paragraph::new(vec![
        Line::from(Span::styled(message, Theme::status_warning())),
        Line::from(Span::styled("q quit", Theme::dimmed())),
    ])
    .alignment(Alignment::Center);
    let y = area.y + area.height.saturating_sub(2) / 2;
    frame.render_widget(notice, Rect::new(area.x, y, area.width, 2));
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ratatui::{backend::TestBackend, Terminal};

    use super::super::layout_tokens::{ATTENTION_MAX_HEIGHT, TABLE_MIN_HEIGHT};
    use super::*;
    use crate::commands::status::data::StatusData;
    use crate::commands::status::ui::tui::state::TuiActivityLog;

    #[test]
    fn budget_leaves_the_remainder_for_the_table() {
        // Nine entry lines: three entries of header, evidence and command.
        let budget = budget(40, 8, 9, 8, false, 0);

        assert_eq!(budget.alerts, 4);
        assert_eq!(budget.attention, 10);
        assert_eq!(budget.activity, 6);
        assert_eq!(budget.table, 10);
    }

    #[test]
    fn attention_budget_fits_a_review_entry_and_its_three_commands() {
        // Header plus the three `loom stage human-review` command lines.
        let budget = budget(40, 0, 4, 0, false, 0);

        assert_eq!(budget.attention, 5);
    }

    #[test]
    fn attention_budget_fits_a_blocked_entry_with_all_its_alternatives() {
        // Header, three note lines, the retry and when to run it, three
        // alternatives of two lines each, and the acceptance pointer.
        let budget = budget(60, 0, 13, 0, false, 0);

        assert_eq!(budget.attention, ATTENTION_MAX_HEIGHT);
        assert_eq!(budget.attention, 14);
        assert_eq!(super::budget(60, 0, 30, 0, false, 0).attention, 14);
    }

    #[test]
    fn budget_preserves_a_minimum_table_before_panels() {
        let budget = budget(16, 0, 6, 3, false, 0);

        assert_eq!(budget.activity, 0);
        assert_eq!(budget.attention, 4);
        assert!(budget.table >= TABLE_MIN_HEIGHT);
    }

    #[test]
    fn footer_takes_a_second_row_only_with_quota_data() {
        let without = budget(40, 0, 0, 0, false, 0);
        let with = budget(40, 0, 0, 0, true, 0);

        assert_eq!(without.footer, 1);
        assert_eq!(with.footer, 2);
        assert_eq!(with.table, without.table - 1);
    }

    #[test]
    fn quota_line_yields_to_the_table_at_minimum_height() {
        // Header 4 + gap 1 + full alert band 4 + gap 1 + footer 2 = 12 reserved rows
        // even in compact mode, leaving 4 for the table at height 16; the footer then
        // drops back to one row so the table matches the no-quota budget.
        let with_alerts = budget(16, 4, 0, 0, true, 0);
        assert_eq!(with_alerts.footer, 1);
        assert_eq!(with_alerts.table, budget(16, 4, 0, 0, false, 0).table);

        // At 17 the two-row footer would still leave the table one short; at 18 the
        // quota line has room again above a minimum-height table.
        let still_short = budget(17, 4, 0, 0, true, 0);
        assert_eq!(still_short.footer, 1);
        assert_eq!(still_short.table, TABLE_MIN_HEIGHT);
        let roomy = budget(18, 4, 0, 0, true, 0);
        assert_eq!(roomy.footer, 2);
        assert_eq!(roomy.table, TABLE_MIN_HEIGHT);
    }

    #[test]
    fn budget_at_minimum_height_with_a_full_alert_band() {
        // Spec gap: `compact` (the third shrink step) is not covered by the two-step
        // shrink the spec describes, and the alert band never shrinks. At this height
        // with a full alert band the table drops below TABLE_MIN_HEIGHT. Nothing panics
        // and ratatui clips safely, but this pins the actual behaviour rather than the
        // spec's minimum.
        let no_panels = budget(16, 4, 0, 0, false, 0);
        assert_eq!(no_panels.alerts, 4);
        assert_eq!(no_panels.attention, 0);
        assert_eq!(no_panels.activity, 0);
        assert_eq!(no_panels.table, 5);
        assert!(
            no_panels.alerts + no_panels.table + no_panels.attention + no_panels.activity <= 16
        );

        let with_panels = budget(16, 4, 6, 3, false, 0);
        assert_eq!(with_panels.alerts, 4);
        assert_eq!(with_panels.attention, 4);
        assert_eq!(with_panels.activity, 0);
        assert_eq!(with_panels.table, 1);
        assert!(
            with_panels.alerts + with_panels.table + with_panels.attention + with_panels.activity
                <= 16
        );
    }

    #[test]
    fn pane_rows_and_their_gap_come_out_of_the_table() {
        let without = budget(40, 0, 0, 0, false, 0);
        let with = budget(40, 0, 0, 0, false, 5);

        assert_eq!(with.tokens, 5);
        assert_eq!(with.table, without.table - 6);
    }

    #[test]
    fn pane_is_dropped_before_the_activity_panel() {
        // Thirteen rows stay for the table without the pane; with eight pane rows
        // and their gap only four would, so the pane goes and activity stays.
        let budget = budget(24, 0, 0, 2, false, 8);

        assert_eq!(budget.tokens, 0);
        assert_eq!(budget.activity, 3);
        assert!(budget.table >= TABLE_MIN_HEIGHT);
    }

    #[test]
    fn pane_is_kept_when_the_table_still_has_its_minimum() {
        let budget = budget(40, 0, 0, 2, false, 8);

        assert_eq!(budget.tokens, 8);
        assert_eq!(budget.activity, 3);
    }

    #[test]
    fn notice_below_minimum() {
        let data = StatusData::default();
        let levels = HashMap::new();
        let ordered = Vec::new();
        let attention = Vec::new();
        let activity = TuiActivityLog::new();
        let alerts = Vec::new();
        let view = LedgerView {
            data: &data,
            levels: &levels,
            ordered: &ordered,
            attention: &attention,
            activity: &activity,
            alerts: &alerts,
            spinner: '⠋',
            scroll_y: 0,
            legend_open: false,
            tick_age_secs: None,
            last_error: None,
            now_epoch: 1_788_523_200,
            scrollable: false,
            show_tokens: false,
        };
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

        terminal
            .draw(|frame| assert_eq!(render(frame, &view), RenderOutcome::default()))
            .unwrap();

        let text: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("at least 64×16"));
    }
}
