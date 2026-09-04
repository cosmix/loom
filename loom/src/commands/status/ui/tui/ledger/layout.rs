//! Vertical layout and rendering for the live ledger dashboard.

use ratatui::{
    layout::{Alignment, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{columns, header, legend, panels, rows, LedgerView, RenderOutcome, MIN_COLS, MIN_ROWS};
use crate::commands::status::ui::theme::Theme;
use crate::commands::status::ui::tui::renderer;
use crate::models::stage::StageStatus;

const HEADER_HEIGHT: u16 = 4;
const FOOTER_HEIGHT: u16 = 1;
const ALERT_MAX_HEIGHT: u16 = 4;
const TABLE_MIN_HEIGHT: u16 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Budget {
    alerts: u16,
    table: u16,
    attention: u16,
    activity: u16,
    footer: u16,
    compact: bool,
}

#[derive(Clone, Copy)]
struct Areas {
    header: Rect,
    alerts: Rect,
    table: Rect,
    attention: Rect,
    activity: Rect,
    footer: Rect,
}

/// Render the responsive live ledger dashboard.
pub fn render(frame: &mut Frame, view: &LedgerView) -> RenderOutcome {
    let area = frame.area();
    if area.width < MIN_COLS || area.height < MIN_ROWS {
        render_size_notice(frame, area);
        return RenderOutcome::default();
    }

    let budget = budget(
        area.height,
        view.alerts.len(),
        view.attention.len(),
        view.activity.len(),
    );
    render_dashboard(frame, area, view, budget);
    if view.legend_open {
        legend::render_legend_overlay(frame, area);
    }
    RenderOutcome {
        table_viewport_rows: budget.table.saturating_sub(2),
    }
}

fn budget(height: u16, alerts: usize, attention_entries: usize, activity_len: usize) -> Budget {
    let alerts = (alerts.min(ALERT_MAX_HEIGHT as usize)) as u16;
    let mut attention = attention_height(attention_entries);
    let mut activity = activity_height(activity_len);
    let mut compact = false;
    let mut table = available_table_height(height, alerts, attention, activity, compact);

    if table < TABLE_MIN_HEIGHT {
        activity = 0;
        table = available_table_height(height, alerts, attention, activity, compact);
    }
    if table < TABLE_MIN_HEIGHT {
        attention = attention.min(4);
        table = available_table_height(height, alerts, attention, activity, compact);
    }
    if table < TABLE_MIN_HEIGHT {
        compact = true;
        table = available_table_height(height, alerts, attention, activity, compact);
    }

    Budget {
        alerts,
        table,
        attention,
        activity,
        footer: FOOTER_HEIGHT,
        compact,
    }
}

fn attention_height(entries: usize) -> u16 {
    if entries == 0 {
        0
    } else {
        entries.saturating_mul(3).saturating_add(1).min(10) as u16
    }
}

fn activity_height(entries: usize) -> u16 {
    if entries == 0 {
        0
    } else {
        (1 + entries).min(6) as u16
    }
}

fn available_table_height(
    height: u16,
    alerts: u16,
    attention: u16,
    activity: u16,
    compact: bool,
) -> u16 {
    let alert_gap = if alerts > 0 { 1 } else { 0 };
    let table_gap = if compact { 0 } else { 1 };
    let attention_gap = if attention > 0 && !compact { 1 } else { 0 };
    let activity_gap = if activity > 0 { 1 } else { 0 };
    let reserved = HEADER_HEIGHT
        + 1
        + alerts
        + alert_gap
        + table_gap
        + attention
        + attention_gap
        + activity
        + activity_gap
        + FOOTER_HEIGHT;
    height.saturating_sub(reserved)
}

fn render_dashboard(frame: &mut Frame, area: Rect, view: &LedgerView, budget: Budget) {
    let areas = areas(area, budget);
    header::render_header(frame, areas.header, view);
    renderer::render_scheduler_alerts(frame, areas.alerts, view.alerts);
    render_table(frame, areas.table, view);
    panels::render_attention(frame, areas.attention, view.attention);
    panels::render_activity(frame, areas.activity, view.activity);

    let present = present_statuses(view);
    panels::render_footer(frame, areas.footer, &present, view.last_error);
}

fn areas(area: Rect, budget: Budget) -> Areas {
    let mut y = area.y;
    let header = take(area, &mut y, HEADER_HEIGHT);
    y += 1;
    let alerts = take(area, &mut y, budget.alerts);
    y += if budget.alerts > 0 { 1 } else { 0 };
    let table = take(area, &mut y, budget.table);
    y += if budget.compact { 0 } else { 1 };
    let attention = take(area, &mut y, budget.attention);
    y += if budget.attention > 0 && !budget.compact {
        1
    } else {
        0
    };
    let activity = take(area, &mut y, budget.activity);
    y += if budget.activity > 0 { 1 } else { 0 };
    let footer = take(area, &mut y, budget.footer);

    Areas {
        header,
        alerts,
        table,
        attention,
        activity,
        footer,
    }
}

fn take(area: Rect, y: &mut u16, height: u16) -> Rect {
    let rect = Rect::new(area.x, *y, area.width, height);
    *y = y.saturating_add(height);
    rect
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
                rows::stage_row(stage, level, &columns, view.ordered)
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

    use super::*;
    use crate::commands::status::data::StatusData;
    use crate::commands::status::ui::tui::state::TuiActivityLog;

    #[test]
    fn budget_leaves_the_remainder_for_the_table() {
        let budget = budget(40, 8, 3, 8);

        assert_eq!(budget.alerts, 4);
        assert_eq!(budget.attention, 10);
        assert_eq!(budget.activity, 6);
        assert_eq!(budget.table, 10);
    }

    #[test]
    fn budget_preserves_a_minimum_table_before_panels() {
        let budget = budget(16, 0, 2, 3);

        assert_eq!(budget.activity, 0);
        assert_eq!(budget.attention, 4);
        assert!(budget.table >= TABLE_MIN_HEIGHT);
    }

    #[test]
    fn budget_at_minimum_height_with_a_full_alert_band() {
        // Spec gap: `compact` (the third shrink step) is not covered by the two-step
        // shrink the spec describes, and the alert band never shrinks. At this height
        // with a full alert band the table drops below TABLE_MIN_HEIGHT. Nothing panics
        // and ratatui clips safely, but this pins the actual behaviour rather than the
        // spec's minimum.
        let no_panels = budget(16, 4, 0, 0);
        assert_eq!(no_panels.alerts, 4);
        assert_eq!(no_panels.attention, 0);
        assert_eq!(no_panels.activity, 0);
        assert_eq!(no_panels.table, 5);
        assert!(
            no_panels.alerts + no_panels.table + no_panels.attention + no_panels.activity <= 16
        );

        let with_panels = budget(16, 4, 2, 3);
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
