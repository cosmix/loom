use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::theme::Theme;
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
    match status {
        StageStatus::Completed => Span::styled("✓", Theme::status_completed()),
        StageStatus::Executing => Span::styled("●", Theme::status_executing()),
        StageStatus::Queued => Span::styled("▶", Theme::status_queued()),
        StageStatus::WaitingForDeps => Span::styled("○", Theme::status_pending()),
        StageStatus::Blocked => Span::styled("✗", Theme::status_blocked()),
        StageStatus::NeedsHandoff => Span::styled("⟳", Theme::status_warning()),
        StageStatus::WaitingForInput => Span::styled("?", Theme::status_warning()),
        StageStatus::Skipped => Span::styled("⊘", Theme::dimmed()),
        StageStatus::MergeConflict => Span::styled("⚡", Theme::status_blocked()),
        StageStatus::CompletedWithFailures => Span::styled("⚠", Theme::status_warning()),
        StageStatus::MergeBlocked => Span::styled("⊗", Theme::status_blocked()),
    }
}

/// Get status text description
pub fn status_text(status: &StageStatus) -> &'static str {
    match status {
        StageStatus::Completed => "Completed",
        StageStatus::Executing => "Executing",
        StageStatus::Queued => "Queued",
        StageStatus::WaitingForDeps => "Waiting",
        StageStatus::Blocked => "Blocked",
        StageStatus::NeedsHandoff => "Handoff",
        StageStatus::WaitingForInput => "Input",
        StageStatus::Skipped => "Skipped",
        StageStatus::MergeConflict => "Conflict",
        StageStatus::CompletedWithFailures => "Failed",
        StageStatus::MergeBlocked => "MergeErr",
    }
}

/// Merged status indicator
pub fn merged_indicator(merged: bool) -> Span<'static> {
    if merged {
        Span::styled("✓", Theme::status_merged())
    } else {
        Span::styled("○", Theme::dimmed())
    }
}
