//! Rendering functions for TUI components.

use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::daemon::CompletionSummary;
use crate::orchestrator::scheduling_report::{Alert, Severity};
use crate::utils::format_elapsed;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};
pub fn render_scheduler_alerts(frame: &mut Frame, area: Rect, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }
    let lines: Vec<Line> = alerts
        .iter()
        .map(|alert| {
            let color = match alert.severity {
                Severity::Critical => StatusColors::BLOCKED,
                Severity::Warning => StatusColors::PENDING,
                Severity::Info => StatusColors::EXECUTING,
            };
            let style = match alert.severity {
                Severity::Critical => Style::default().fg(color).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(color),
            };
            Line::from(vec![
                Span::styled(format!("   {} ", alert.severity.marker()), style),
                Span::styled(alert.text.clone(), style),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Render completion screen with summary of all stages.
pub fn render_completion(frame: &mut Frame, area: Rect, summary: &CompletionSummary) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::text::{Line, Span};

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Header
            Constraint::Length(2), // Summary
            Constraint::Min(10),   // Stage table
            Constraint::Length(1), // Footer
        ])
        .split(area);

    // Header
    let success = summary.failure_count == 0;
    let header_text = if success {
        Line::from(vec![
            Span::styled("\u{2713} ", Style::default().fg(StatusColors::COMPLETED)),
            Span::styled(
                "Orchestration Complete",
                Style::default()
                    .fg(StatusColors::COMPLETED)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        Line::from(vec![
            Span::styled("\u{2717} ", Style::default().fg(StatusColors::BLOCKED)),
            Span::styled(
                "Orchestration Complete (with failures)",
                Style::default()
                    .fg(StatusColors::BLOCKED)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    };

    let total_time = format_elapsed(summary.total_duration_secs);
    let summary_line = Line::from(vec![
        Span::styled("Total: ", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(total_time),
        Span::raw(" | "),
        Span::styled("\u{2713} ", Style::default().fg(StatusColors::COMPLETED)),
        Span::raw(summary.success_count.to_string()),
        Span::raw(" | "),
        Span::styled("\u{2717} ", Style::default().fg(StatusColors::BLOCKED)),
        Span::raw(summary.failure_count.to_string()),
    ]);

    let header_block = Block::default()
        .title(" Orchestration Results ")
        .title_style(Theme::header())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    let header_content =
        Paragraph::new(vec![header_text, Line::from(""), summary_line]).block(header_block);
    frame.render_widget(header_content, chunks[0]);

    // Sort stages by completion (completed first, then by id)
    let mut sorted_stages = summary.stages.clone();
    sorted_stages.sort_by(|a, b| match (&a.duration_secs, &b.duration_secs) {
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => a.id.cmp(&b.id),
    });

    // Stage table
    let table_block = Block::default()
        .title(format!(" Stages ({}) ", sorted_stages.len()))
        .title_style(Theme::header())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    let header = Row::new(vec!["", "Stage", "Status", "Duration"])
        .style(Theme::header())
        .bottom_margin(1);

    let rows: Vec<Row> = sorted_stages
        .iter()
        .map(|stage| {
            let icon = stage.status.icon();
            let status_str = stage.status.label();
            let style = stage.status.tui_style();

            let duration = stage
                .duration_secs
                .map(format_elapsed)
                .unwrap_or_else(|| "-".to_string());

            let id_display = crate::utils::truncate(&stage.id, 30);

            Row::new(vec![
                icon.to_string(),
                id_display,
                status_str.to_string(),
                duration,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(2),
        ratatui::layout::Constraint::Min(20),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(8),
    ];

    let table = Table::new(rows, widths).block(table_block).header(header);
    frame.render_widget(table, chunks[2]);

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" quit"),
    ]));
    frame.render_widget(footer, chunks[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::StageCompletionInfo;
    use crate::models::stage::StageStatus;
    use ratatui::{backend::TestBackend, Terminal};

    fn completion_summary(id: &str) -> CompletionSummary {
        CompletionSummary {
            total_duration_secs: 90,
            stages: vec![StageCompletionInfo {
                id: id.to_owned(),
                name: id.to_owned(),
                status: StageStatus::Completed,
                duration_secs: Some(90),
                execution_secs: Some(90),
                retry_count: 0,
                merged: true,
                dependencies: Vec::new(),
            }],
            success_count: 1,
            failure_count: 0,
            plan_path: "plan.md".to_owned(),
        }
    }

    fn completion_screen(id: &str) -> String {
        let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_completion(frame, area, &completion_summary(id));
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn completion_ids_use_a_thirty_character_budget() {
        for ch in ["x", "é"] {
            for length in [27, 28, 29, 30] {
                let id = ch.repeat(length);
                let screen = completion_screen(&id);
                assert!(screen.contains(&id), "output: {screen}");
                assert!(!screen.contains("..."), "output: {screen}");
            }
            let id = ch.repeat(31);
            let expected = format!("{}...", ch.repeat(27));
            let screen = completion_screen(&id);
            assert!(screen.contains(&expected), "output: {screen}");
            assert!(!screen.contains(&id), "output: {screen}");
        }
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(30), "30s");
        assert_eq!(format_elapsed(90), "1m30s");
        assert_eq!(format_elapsed(3661), "1h1m");
    }
}
