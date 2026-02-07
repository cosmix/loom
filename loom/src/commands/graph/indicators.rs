//! Status indicators for graph display
//!
//! Provides colored status indicators and priority ordering for stage statuses.

use colored::{ColoredString, Colorize};

use crate::models::stage::StageStatus;

/// Status indicator with color for display
pub fn status_indicator(status: &StageStatus) -> ColoredString {
    let icon = status.icon();
    let color = status.terminal_color();
    let mut colored = icon.color(color);

    if status.is_bold() {
        colored = colored.bold();
    }
    if status.is_dimmed() {
        colored = colored.dimmed();
    }
    if status.is_strikethrough() {
        colored = colored.strikethrough();
    }

    colored
}

/// Sort stages by status priority (executing first, then ready, then others)
pub fn status_priority(status: &StageStatus) -> u8 {
    match status {
        StageStatus::Executing => 0,
        StageStatus::Queued => 1,
        StageStatus::WaitingForInput => 2,
        StageStatus::NeedsHandoff => 3,
        StageStatus::MergeConflict => 4, // Needs attention, similar to handoff
        StageStatus::CompletedWithFailures => 5, // Needs retry
        StageStatus::MergeBlocked => 6,  // Merge failed, needs attention
        StageStatus::NeedsHumanReview => 7, // Needs human attention
        StageStatus::WaitingForDeps => 8,
        StageStatus::Blocked => 9,
        StageStatus::Completed => 10,
        StageStatus::Skipped => 11,
    }
}
