//! Owns the dashboard's vertical band split and the adaptive activity-log height.

use std::rc::Rc;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Minimum height for the activity log area.
const ACTIVITY_MIN_HEIGHT: u16 = 5;

/// Maximum height for the activity log area.
const ACTIVITY_MAX_HEIGHT: u16 = 10;

/// Maximum rows the scheduler alert band may occupy.
///
/// Bounded so a plan with many blocked stages cannot crowd out the graph; the
/// static `loom status` prints the full list.
const ALERT_MAX_HEIGHT: u16 = 4;

/// Split `area` into the vertical bands `render` draws into.
///
/// Bundles the activity-log/alert height arithmetic with the layout
/// split so `render` doesn't have to carry it inline; also returns the
/// activity-log height, which `render` needs again afterward to bound
/// the graph's scroll viewport.
pub(super) fn layout_chunks(
    area: Rect,
    activity_count: usize,
    alert_count: usize,
) -> (Rc<[Rect]>, u16) {
    // Calculate activity log height (adaptive)
    let activity_height = if activity_count == 0 {
        ACTIVITY_MIN_HEIGHT
    } else {
        // Inner height = entries shown + 2 (borders)
        ((activity_count as u16) + 2).clamp(ACTIVITY_MIN_HEIGHT, ACTIVITY_MAX_HEIGHT)
    };

    // The alert band replaces the header spacer: one line when quiet, one
    // line per alert (capped) when the scheduler has something to say.
    let alert_height = (alert_count as u16).clamp(1, ALERT_MAX_HEIGHT);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),               // Header (logo + progress)
            Constraint::Length(alert_height),    // Scheduler alerts / spacer
            Constraint::Min(6),                  // Graph (adaptive, takes remaining)
            Constraint::Length(1),               // Spacer
            Constraint::Length(activity_height), // Activity log
            Constraint::Length(1),               // Footer
        ])
        .split(area);

    (chunks, activity_height)
}
