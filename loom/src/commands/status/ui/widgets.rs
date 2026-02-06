use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem};

use super::theme::{StatusColors, Theme};
use crate::commands::status::data::ActivityStatus;
use crate::models::stage::StageStatus;

/// Unicode block characters for progress bars
const BLOCKS: [char; 9] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];

/// Create a progress bar using Unicode block characters
///
/// # Arguments
/// * `pct` - Progress percentage (0.0 to 1.0)
/// * `width` - Total width in characters
/// * `style` - Style for filled portion
pub fn progress_bar(pct: f32, width: usize, _style: Style) -> String {
    let pct = pct.clamp(0.0, 1.0);
    let filled_width = pct * width as f32;
    let full_blocks = filled_width.floor() as usize;
    let partial_idx = ((filled_width - full_blocks as f32) * 8.0).round() as usize;

    let mut bar = String::with_capacity(width);

    // Full blocks
    for _ in 0..full_blocks {
        bar.push(BLOCKS[8]);
    }

    // Partial block
    if full_blocks < width && partial_idx > 0 {
        bar.push(BLOCKS[partial_idx]);
    }

    // Empty space
    let current_len = bar.chars().count();
    for _ in current_len..width {
        bar.push(BLOCKS[0]);
    }

    bar
}

/// Create a mini context bar for tables (compact version)
///
/// # Arguments
/// * `pct` - Context percentage (0.0 to 1.0)
/// * `width` - Bar width (default: 8)
pub fn context_bar(pct: f32, width: usize) -> Line<'static> {
    let style = Theme::context_style(pct);
    let bar = progress_bar(pct, width, style);
    let pct_str = format!("{:3.0}%", pct * 100.0);

    Line::from(vec![
        Span::styled(bar, style),
        Span::raw(" "),
        Span::styled(pct_str, style),
    ])
}

/// Get status indicator symbol with color
///
/// Returns a styled Span with the appropriate Unicode symbol
pub fn status_indicator(status: &StageStatus) -> Span<'static> {
    Span::styled(status.icon().to_string(), status.tui_style())
}

/// Get status text description
pub fn status_text(status: &StageStatus) -> &'static str {
    status.label()
}

/// Merged status indicator
pub fn merged_indicator(merged: bool) -> Span<'static> {
    if merged {
        Span::styled("✓", Theme::status_merged())
    } else {
        Span::styled("○", Theme::dimmed())
    }
}

/// Get activity status indicator with appropriate styling
pub fn activity_indicator(status: &ActivityStatus) -> Span<'static> {
    match status {
        ActivityStatus::Idle => Span::styled("\u{23F3} IDLE", Theme::dimmed()),
        ActivityStatus::Working => Span::styled("\u{1F504} WORKING", Theme::status_executing()),
        ActivityStatus::Error => Span::styled("\u{274C} ERROR", Theme::status_blocked()),
        ActivityStatus::Stale => Span::styled("\u{26A0} STALE", Theme::status_warning()),
    }
}

/// Create a context budget gauge with threshold coloring
pub fn context_budget_gauge(usage_pct: f32, budget_pct: f32) -> Gauge<'static> {
    let color = if usage_pct >= 65.0 {
        StatusColors::BLOCKED
    } else if usage_pct >= 50.0 {
        StatusColors::WARNING
    } else {
        StatusColors::COMPLETED
    };

    Gauge::default()
        .percent(usage_pct.clamp(0.0, 100.0) as u16)
        .gauge_style(Style::default().fg(color))
        .label(format!("{usage_pct:.0}% (budget: {budget_pct:.0}%)"))
}

/// Create an activity feed widget displaying recent activities with status
///
/// # Arguments
/// * `activities` - Slice of (message, status) tuples
/// * `title` - Widget title
pub fn activity_feed_widget<'a>(
    activities: &'a [(String, ActivityStatus)],
    title: &'a str,
) -> List<'a> {
    let items: Vec<ListItem> = activities
        .iter()
        .map(|(msg, status)| {
            let style = match status {
                ActivityStatus::Working => Theme::status_executing(),
                ActivityStatus::Error => Theme::status_blocked(),
                ActivityStatus::Stale => Theme::status_warning(),
                ActivityStatus::Idle => Theme::dimmed(),
            };
            ListItem::new(format!("{} {}", status.icon(), msg)).style(style)
        })
        .collect();

    List::new(items).block(Block::default().borders(Borders::ALL).title(title))
}
