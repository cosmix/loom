//! Rendering functions for TUI components.

use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame,
};

use super::state::UnifiedStage;
use crate::commands::status::ui::theme::{StatusColors, Theme};
use crate::commands::status::ui::tree_widget::TreeWidget;
use crate::commands::status::ui::widgets::{status_indicator, status_text};
use crate::models::stage::{Stage, StageStatus};

/// Fixed height for the graph area (prevents jerking from dynamic resizing).
pub const GRAPH_AREA_HEIGHT: u16 = 12;

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
    format!("[{}{}]", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

/// Convert UnifiedStage to Stage for graph widget compatibility.
pub fn unified_stage_to_stage(us: &UnifiedStage) -> Stage {
    use chrono::Utc;

    Stage {
        id: us.id.clone(),
        name: us.id.clone(),
        description: None,
        status: us.status.clone(),
        dependencies: us.dependencies.clone(),
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
        created_at: us.started_at.unwrap_or_else(Utc::now),
        updated_at: Utc::now(),
        completed_at: us.completed_at,
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
        merged: us.merged,
        merge_conflict: false,
    }
}

/// Render the tree-based execution graph.
pub fn render_tree_graph(
    frame: &mut Frame,
    area: Rect,
    stages: &[Stage],
    scroll_y: u16,
    context_pcts: &HashMap<String, f32>,
    elapsed_times: &HashMap<String, i64>,
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

    let tree_widget = TreeWidget::new(stages)
        .context_percentages(context_pcts.clone())
        .elapsed_times(elapsed_times.clone());

    let lines = tree_widget.build_lines();
    let visible_lines: Vec<_> = lines.into_iter().skip(scroll_y as usize).collect();
    let paragraph = Paragraph::new(visible_lines);
    frame.render_widget(paragraph, inner_area);
}

/// Render unified stage table with all columns.
pub fn render_unified_table(frame: &mut Frame, area: Rect, stages: &[UnifiedStage]) {
    let block = Block::default()
        .title(format!(" Stages ({}) ", stages.len()))
        .title_style(Theme::header())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    if stages.is_empty() {
        let empty = Paragraph::new("No stages")
            .style(Theme::dimmed())
            .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec!["", "Lvl", "ID", "Deps", "Status", "Merged", "Elapsed"])
        .style(Theme::header())
        .bottom_margin(1);

    let rows: Vec<Row> = stages
        .iter()
        .map(|stage| {
            let icon = status_indicator(&stage.status);
            let status_str = status_text(&stage.status);
            let merged_str = if stage.merged { "\u{2713}" } else { "\u{25CB}" };

            let level_str = stage.level.to_string();

            let elapsed_str = match (&stage.status, stage.started_at, stage.completed_at) {
                (StageStatus::Executing, Some(start), _) => {
                    let elapsed = chrono::Utc::now()
                        .signed_duration_since(start)
                        .num_seconds();
                    format_elapsed(elapsed)
                }
                (_, Some(start), Some(end)) => {
                    let elapsed = end.signed_duration_since(start).num_seconds();
                    format_elapsed(elapsed)
                }
                _ => "-".to_string(),
            };

            let style = match stage.status {
                StageStatus::Executing => Theme::status_executing(),
                StageStatus::Completed => Theme::status_completed(),
                StageStatus::Blocked | StageStatus::MergeConflict | StageStatus::MergeBlocked => {
                    Theme::status_blocked()
                }
                StageStatus::NeedsHandoff
                | StageStatus::WaitingForInput
                | StageStatus::CompletedWithFailures => Theme::status_warning(),
                StageStatus::Queued => Theme::status_queued(),
                _ => Theme::dimmed(),
            };

            let deps_str = format_dependencies(&stage.dependencies, 20);

            Row::new(vec![
                icon.content.to_string(),
                level_str,
                stage.id.clone(),
                deps_str,
                status_str.to_string(),
                merged_str.to_string(),
                elapsed_str,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        ratatui::layout::Constraint::Length(2),
        ratatui::layout::Constraint::Length(3),
        ratatui::layout::Constraint::Min(15),
        ratatui::layout::Constraint::Length(20),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(6),
        ratatui::layout::Constraint::Length(8),
    ];

    let table = Table::new(rows, widths).block(block).header(header);
    frame.render_widget(table, area);
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
            Span::styled("\u{2191}\u{2193}\u{2190}\u{2192}", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" scroll \u{2502} "),
            Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" page \u{2502} "),
            Span::styled("Home/End", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" jump"),
        ])
    };

    let footer = Paragraph::new(line);
    frame.render_widget(footer, area);
}

/// Format elapsed time in human-readable format.
pub fn format_elapsed(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// Format dependencies as "(dep1, dep2, ...)" with middle truncation if too long.
pub fn format_dependencies(deps: &[String], max_width: usize) -> String {
    if deps.is_empty() {
        return "-".to_string();
    }

    let inner = deps.join(", ");
    let full = format!("({inner})");

    if full.len() <= max_width {
        return full;
    }

    if max_width <= 5 {
        return "...".to_string();
    }

    let available = max_width - 5;
    let left_len = available.div_ceil(2);
    let right_len = available / 2;

    let left: String = inner.chars().take(left_len).collect();
    let right: String = inner.chars().skip(inner.len().saturating_sub(right_len)).collect();

    format!("({left}...{right})")
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
    fn test_format_dependencies() {
        let empty: Vec<String> = vec![];
        assert_eq!(format_dependencies(&empty, 20), "-");

        let single = vec!["stage-a".to_string()];
        assert_eq!(format_dependencies(&single, 20), "(stage-a)");

        let multi = vec!["a".to_string(), "b".to_string()];
        assert_eq!(format_dependencies(&multi, 20), "(a, b)");

        let long = vec![
            "knowledge-bootstrap".to_string(),
            "implement-feature".to_string(),
        ];
        let result = format_dependencies(&long, 20);
        assert!(result.starts_with('('));
        assert!(result.ends_with(')'));
        assert!(result.contains("..."));
        assert!(result.len() <= 20);

        let tiny_result = format_dependencies(&long, 5);
        assert_eq!(tiny_result, "...");
    }

    #[test]
    fn test_unified_stage_to_stage_conversion() {
        let unified = UnifiedStage {
            id: "test-stage".to_string(),
            status: StageStatus::Executing,
            merged: true,
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
            level: 2,
            dependencies: vec!["dep1".to_string(), "dep2".to_string()],
        };

        let stage = unified_stage_to_stage(&unified);

        assert_eq!(stage.id, "test-stage");
        assert_eq!(stage.status, StageStatus::Executing);
        assert!(stage.merged);
        assert_eq!(stage.dependencies, vec!["dep1".to_string(), "dep2".to_string()]);
    }
}
