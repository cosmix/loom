//! Rendering functions for TUI components.

use super::state::TuiActivityLog;
use crate::commands::status::data::StageSummary;
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::commands::status::ui::tree_widget::TreeWidget;
use crate::daemon::CompletionSummary;
use crate::models::stage::{Implementers, Stage};
use crate::orchestrator::scheduling_report::{Alert, Severity};
use crate::utils::format_elapsed;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};
pub fn render_compact_header(
    frame: &mut Frame,
    area: Rect,
    spinner: char,
    pct: f64,
    completed_count: usize,
    total: usize,
    plan_name: Option<&str>,
) {
    let progress_str = format!("{completed_count}/{total} ({:.0}%)", pct * 100.0);
    let mut lines: Vec<Line> = crate::LOGO
        .lines()
        .map(|l| Line::from(Span::styled(l, Theme::header())))
        .collect();
    let mut progress_spans = vec![
        Span::styled(format!("   {spinner} "), Theme::header()),
        Span::styled(progress_str, Style::default().fg(StatusColors::COMPLETED)),
        Span::raw(" "),
        Span::styled(progress_bar_compact(pct, 20), Theme::status_completed()),
    ];
    if let Some(name) = plan_name {
        progress_spans.push(Span::raw("  "));
        progress_spans.push(Span::styled(name, Theme::dimmed()));
    }
    lines.push(Line::from(progress_spans));
    let header = Paragraph::new(lines);
    frame.render_widget(header, area);
}
pub fn render_scheduler_alerts(frame: &mut Frame, area: Rect, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }
    let lines: Vec<Line> = alerts
        .iter()
        .map(|alert| {
            let (marker, color) = match alert.severity {
                Severity::Critical => ("\u{2716} ", StatusColors::BLOCKED),
                Severity::Warning => ("! ", StatusColors::PENDING),
                Severity::Info => ("\u{00b7} ", StatusColors::EXECUTING),
            };
            let style = match alert.severity {
                Severity::Critical => Style::default().fg(color).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(color),
            };
            Line::from(vec![
                Span::styled(format!("   {marker}"), style),
                Span::styled(alert.text.clone(), style),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}
fn progress_bar_compact(pct: f64, width: usize) -> String {
    let filled = (pct * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!(
        "[{}{}]",
        "\u{2588}".repeat(filled),
        "\u{2591}".repeat(empty)
    )
}

pub fn stage_summary_to_stage(summary: &StageSummary) -> Stage {
    use chrono::Utc;

    let started_at = Utc::now() - chrono::Duration::seconds(summary.elapsed_secs.unwrap_or(0));
    Stage {
        id: summary.id.clone(),
        name: summary.name.clone(),
        description: None,
        code_review: None,
        status: summary.status.clone(),
        dependencies: summary.dependencies.clone(),
        parallel_group: None,
        acceptance: vec![],
        setup: vec![],
        files: vec![],
        stage_type: summary.stage_type,
        plan_id: None,
        worktree: None,
        session: None,
        held: summary.held,
        parent_stage: None,
        child_stages: vec![],
        created_at: started_at,
        updated_at: Utc::now(),
        completed_at: None,
        started_at: Some(started_at),
        duration_secs: summary.execution_secs,
        execution_secs: summary.execution_secs,
        attempt_started_at: None,
        close_reason: None,
        auto_merge: None,
        working_dir: None,
        retry_count: summary.retry_count,
        max_retries: summary.max_retries,
        last_failure_at: None,
        failure_info: summary.failure_info.clone(),
        resolved_base: None,
        base_branch: summary.base_branch.clone(),
        base_merged_from: summary.base_merged_from.clone(),
        outputs: vec![],
        completed_commit: None,
        cleanup_warning: summary.cleanup_warning.clone(),
        merged: summary.merged,
        merge_conflict: false,
        verification_status: Default::default(),
        context_ceiling_tokens: summary.context_ceiling_tokens,
        plan_overview: None,
        artifacts: vec![],
        wiring: vec![],
        wiring_tests: vec![],
        dead_code_check: None,
        before_stage: vec![],
        after_stage: vec![],
        fix_attempts: 0,
        dispute_count: summary.dispute_count,
        evidence_rounds: 0,
        amendments_applied: 0,
        stall_recoveries: 0,
        sandbox: Default::default(),
        execution_mode: None,
        max_fix_attempts: None,
        review_reason: summary.review_reason.clone(),
        bug_fix: None,
        regression_test: None,
        model: (!summary.model.is_empty()).then(|| summary.model.clone()),
        reasoning_effort: None,
        ultracode: false,
        implementers: Implementers::default(),
        subagent_timeout_secs: None,
    }
}

pub fn render_tree_graph(frame: &mut Frame, area: Rect, stages: &[Stage], scroll_y: u16) {
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

pub fn render_activity_log(frame: &mut Frame, area: Rect, activity: &TuiActivityLog) {
    let block = Block::default()
        .title(" Activity ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));
    let inner = block.inner(area);
    let max_lines = inner.height as usize;
    if activity.is_empty() {
        let empty =
            Paragraph::new(Span::styled("Waiting for events...", Theme::dimmed())).block(block);
        frame.render_widget(empty, area);
        return;
    }
    let lines = activity.render_lines(max_lines);
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

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
            let icon = stage.status.icon();
            let status_str = stage.status.label();
            let style = stage.status.tui_style();

            let duration = stage
                .duration_secs
                .map(format_elapsed)
                .unwrap_or_else(|| "-".to_string());

            // UTF-8-safe truncation (fixes byte-slice panic on multi-byte IDs)
            let id_display = {
                let truncated: String = stage.id.chars().take(27).collect();
                if truncated.len() < stage.id.chars().count() {
                    format!("{truncated}...")
                } else {
                    stage.id.clone()
                }
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
    use crate::models::stage::StageStatus;

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(30), "30s");
        assert_eq!(format_elapsed(90), "1m30s");
        assert_eq!(format_elapsed(3661), "1h1m");
    }

    #[test]
    fn test_stage_summary_to_stage_conversion() {
        let summary = StageSummary {
            id: "test-stage".to_string(),
            name: "Test Stage".to_string(),
            status: StageStatus::Executing,
            stage_type: Default::default(),
            dependencies: vec!["dep1".to_string(), "dep2".to_string()],
            context_tokens: None,
            elapsed_secs: None,
            execution_secs: None,
            base_branch: None,
            base_merged_from: vec![],
            failure_info: None,
            activity_status: Default::default(),
            last_tool: None,
            last_activity: None,
            staleness_secs: None,
            context_ceiling_tokens: None,
            review_reason: None,
            merged: true,
            cleanup_warning: None,
            held: false,
            retry_count: 0,
            max_retries: None,
            pid: None,
            session_alive: false,
            model: "opus".to_string(),
            session_type: None,
            incoherence: None,
            execution_models: vec![],
            dispute_count: 0,
            judge_heartbeat_secs: None,
            session_backend: None,
        };

        let stage = stage_summary_to_stage(&summary);

        assert_eq!(stage.id, "test-stage");
        assert_eq!(stage.status, StageStatus::Executing);
        assert!(stage.merged);
        assert_eq!(
            stage.dependencies,
            vec!["dep1".to_string(), "dep2".to_string()]
        );
    }
}
