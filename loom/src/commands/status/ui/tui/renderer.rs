//! Rendering functions for TUI components.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use super::state::TuiActivityLog;
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::commands::status::ui::tree_widget::TreeWidget;
use crate::daemon::{CompletionSummary, StageInfo};
use crate::models::stage::{Stage, StageStatus};
use crate::utils::format_elapsed;

/// Render compact header with logo and inline progress.
pub fn render_compact_header(
    frame: &mut Frame,
    area: Rect,
    spinner: char,
    pct: f64,
    completed_count: usize,
    total: usize,
) {
    let progress_str = format!("{completed_count}/{total} ({:.0}%)", pct * 100.0);

    let mut lines: Vec<Line> = crate::LOGO
        .lines()
        .map(|l| Line::from(Span::styled(l, Theme::header())))
        .collect();

    lines.push(Line::from(vec![
        Span::styled(format!("   {spinner} "), Theme::header()),
        Span::styled(progress_str, Style::default().fg(StatusColors::COMPLETED)),
        Span::raw(" "),
        Span::styled(progress_bar_compact(pct, 20), Theme::status_completed()),
    ]));

    let header = Paragraph::new(lines);
    frame.render_widget(header, area);
}

/// Create a compact progress bar string.
fn progress_bar_compact(pct: f64, width: usize) -> String {
    let filled = (pct * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!(
        "[{}{}]",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty)
    )
}

/// Convert StageInfo (from daemon) to Stage (for tree widget).
pub fn stage_info_to_stage(info: &StageInfo) -> Stage {
    use chrono::Utc;

    Stage {
        id: info.id.clone(),
        name: info.name.clone(),
        description: None,
        status: info.status.clone(),
        dependencies: info.dependencies.clone(),
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        stage_type: Default::default(),
        plan_id: None,
        worktree: None,
        session: None,
        held: false,
        parent_stage: None,
        child_stages: vec![],
        created_at: info.started_at,
        updated_at: Utc::now(),
        completed_at: info.completed_at,
        started_at: Some(info.started_at),
        duration_secs: info.completed_at.map(|end| {
            end.signed_duration_since(info.started_at).num_seconds()
        }),
        execution_secs: None,
        attempt_started_at: None,
        close_reason: None,
        auto_merge: None,
        working_dir: None,
        retry_count: 0,
        max_retries: None,
        last_failure_at: None,
        failure_info: None,
        resolved_base: None,
        base_branch: None,
        base_merged_from: vec![],
        outputs: vec![],
        completed_commit: None,
        merged: info.merged,
        merge_conflict: false,
        verification_status: Default::default(),
        context_budget: None,
        truths: vec![],
        artifacts: vec![],
        wiring: vec![],
        truth_checks: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        fix_attempts: 0,
        sandbox: Default::default(),
        execution_mode: None,
        max_fix_attempts: None,
        review_reason: None,
        bug_fix: None,
        regression_test: None,
    }
}

/// Render the tree-based execution graph.
pub fn render_tree_graph(
    frame: &mut Frame,
    area: Rect,
    stages: &[Stage],
    scroll_y: u16,
) {
    let graph_block = Block::default()
        .title(" Execution Graph ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    let inner_area = graph_block.inner(area);
    frame.render_widget(graph_block, area);

    if stages.is_empty() {
        let empty = Paragraph::new(Span::styled("(no stages)", Theme::dimmed()));
        frame.render_widget(empty, inner_area);
        return;
    }

    let tree_widget = TreeWidget::new(stages).max_width(inner_area.width as usize);
    let lines = tree_widget.build_lines();
    let visible_lines: Vec<_> = lines.into_iter().skip(scroll_y as usize).collect();
    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner_area);
}

/// Render the activity log showing recent state transitions.
pub fn render_activity_log(frame: &mut Frame, area: Rect, activity: &TuiActivityLog) {
    let block = Block::default()
        .title(" Activity ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    let inner = block.inner(area);
    let max_lines = inner.height as usize;

    if activity.is_empty() {
        let empty = Paragraph::new(Span::styled("Waiting for events...", Theme::dimmed()))
            .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let lines = activity.render_lines(max_lines);
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render compact footer with keybinds.
pub fn render_compact_footer(frame: &mut Frame, area: Rect, last_error: &Option<String>) {
    let line = if let Some(ref err) = last_error {
        Line::from(vec![
            Span::styled("Error: ", Style::default().fg(StatusColors::BLOCKED)),
            Span::styled(err.as_str(), Style::default().fg(StatusColors::BLOCKED)),
        ])
    } else {
        Line::from(vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" quit \u{2502} "),
            Span::styled(
                "\u{2191}\u{2193}",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(" scroll \u{2502} "),
            Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" page"),
        ])
    };

    let footer = Paragraph::new(line);
    frame.render_widget(footer, area);
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
            let icon = match stage.status {
                StageStatus::Completed => "\u{2713}",
                StageStatus::Skipped => "\u{2298}",
                StageStatus::Blocked => "\u{2717}",
                StageStatus::MergeConflict => "\u{26A1}",
                StageStatus::CompletedWithFailures => "\u{26A0}",
                StageStatus::MergeBlocked => "\u{2297}",
                _ => "\u{25CB}",
            };

            let duration = stage
                .duration_secs
                .map(format_elapsed)
                .unwrap_or_else(|| "-".to_string());

            let status_str = match stage.status {
                StageStatus::Completed => "Completed",
                StageStatus::Skipped => "Skipped",
                StageStatus::Blocked => "Blocked",
                StageStatus::MergeConflict => "Conflict",
                StageStatus::CompletedWithFailures => "Failed",
                StageStatus::MergeBlocked => "MergeBlk",
                _ => "Other",
            };

            let style = match stage.status {
                StageStatus::Completed => Theme::status_completed(),
                StageStatus::Skipped => Theme::dimmed(),
                StageStatus::Blocked
                | StageStatus::CompletedWithFailures
                | StageStatus::MergeBlocked => Theme::status_blocked(),
                StageStatus::MergeConflict => Theme::status_warning(),
                _ => Theme::dimmed(),
            };

            let id_display = if stage.id.len() > 30 {
                format!("{}...", &stage.id[..27])
            } else {
                stage.id.clone()
            };

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

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(30), "30s");
        assert_eq!(format_elapsed(90), "1m30s");
        assert_eq!(format_elapsed(3661), "1h1m");
    }

    #[test]
    fn test_stage_info_to_stage_conversion() {
        let info = StageInfo {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            session_pid: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            worktree_status: None,
            status: StageStatus::Executing,
            merged: true,
            dependencies: vec!["dep1".to_string(), "dep2".to_string()],
        };

        let stage = stage_info_to_stage(&info);

        assert_eq!(stage.id, "test-stage");
        assert_eq!(stage.status, StageStatus::Executing);
        assert!(stage.merged);
        assert_eq!(
            stage.dependencies,
            vec!["dep1".to_string(), "dep2".to_string()]
        );
    }
}
