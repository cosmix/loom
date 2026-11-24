//! Status indicators for graph display
//!
//! Provides colored status indicators for stage statuses.

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
