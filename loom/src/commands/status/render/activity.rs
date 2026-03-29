//! Activity status rendering helpers

use colored::{ColoredString, Colorize};

use crate::models::constants::STALENESS_THRESHOLD_SECS;

use super::super::data::ActivityStatus;

/// Render activity status as a colored string
pub fn render_activity_status(status: ActivityStatus) -> ColoredString {
    match status {
        ActivityStatus::Idle => "IDLE".dimmed(),
        ActivityStatus::Working => "WORKING".blue().bold(),
        ActivityStatus::Error => "ERROR".red().bold(),
        ActivityStatus::Stale => "STALE".yellow().bold(),
    }
}

/// Render staleness warning if session appears hung
pub fn render_staleness_warning(secs: u64) -> Option<String> {
    if secs > STALENESS_THRESHOLD_SECS {
        let mins = secs / 60;
        Some(format!(
            "  No activity for {mins} minutes - session may be hung"
        ))
    } else {
        None
    }
}
