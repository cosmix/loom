//! Status indicators for graph display
//!
//! Provides colored status indicators and priority ordering for stage statuses.

use colored::{ColoredString, Colorize};

use crate::models::stage::StageStatus;

/// Status indicator with color for display
pub fn status_indicator(status: &StageStatus) -> ColoredString {
    match status {
        StageStatus::Completed => "✓".green().bold(),
        StageStatus::Executing => "●".blue().bold(),
        StageStatus::Queued => "▶".cyan().bold(),
        StageStatus::WaitingForDeps => "○".white().dimmed(),
        StageStatus::WaitingForInput => "?".magenta().bold(),
        StageStatus::Blocked => "✗".red().bold(),
        StageStatus::NeedsHandoff => "⟳".yellow().bold(),
        StageStatus::Skipped => "⊘".white().dimmed().strikethrough(),
        StageStatus::MergeConflict => "⚡".yellow().bold(),
        StageStatus::CompletedWithFailures => "✗".red().bold(),
        StageStatus::MergeBlocked => "⚠".red().bold(),
    }
}

/// Sort stages by status priority (executing first, then ready, then others)
pub fn status_priority(status: &StageStatus) -> u8 {
    match status {
        StageStatus::Executing => 0,
        StageStatus::Queued => 1,
        StageStatus::WaitingForInput => 2,
        StageStatus::NeedsHandoff => 3,
        StageStatus::MergeConflict => 4,           // Needs attention, similar to handoff
        StageStatus::CompletedWithFailures => 5,   // Needs retry
        StageStatus::MergeBlocked => 6,            // Merge failed, needs attention
        StageStatus::WaitingForDeps => 7,
        StageStatus::Blocked => 8,
        StageStatus::Completed => 9,
        StageStatus::Skipped => 10,
    }
}
