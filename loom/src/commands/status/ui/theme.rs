use ratatui::style::{Color, Modifier, Style};

/// Color scheme for status indicators
pub struct StatusColors;

impl StatusColors {
    // Stage status colors
    pub const EXECUTING: Color = Color::Blue;
    pub const COMPLETED: Color = Color::Green;
    pub const BLOCKED: Color = Color::Red;
    pub const PENDING: Color = Color::Gray;
    pub const QUEUED: Color = Color::Cyan;
    pub const WARNING: Color = Color::Yellow;
    pub const HANDOFF: Color = Color::Magenta;
    pub const CONFLICT: Color = Color::Red;
    pub const MERGED: Color = Color::Rgb(100, 180, 100); // Lighter green for merged

    // Context bar colors
    pub const CONTEXT_LOW: Color = Color::Green; // 0-60%
    pub const CONTEXT_MED: Color = Color::Yellow; // 60-75%
    pub const CONTEXT_HIGH: Color = Color::Red; // 75-100%

    // UI chrome
    pub const HEADER: Color = Color::White;
    pub const DIMMED: Color = Color::DarkGray;
    pub const BORDER: Color = Color::Gray;

    // Graph colors
    pub const GRAPH_EDGE: Color = Color::DarkGray;
    pub const GRAPH_NODE_ACTIVE: Color = Color::Blue;
    pub const GRAPH_NODE_DONE: Color = Color::Green;
    pub const GRAPH_NODE_PENDING: Color = Color::Gray;
}

/// Theme provides pre-built styles
pub struct Theme;

impl Theme {
    pub fn header() -> Style {
        Style::default().fg(StatusColors::HEADER).add_modifier(Modifier::BOLD)
    }

    pub fn dimmed() -> Style {
        Style::default().fg(StatusColors::DIMMED)
    }

    pub fn status_executing() -> Style {
        Style::default().fg(StatusColors::EXECUTING).add_modifier(Modifier::BOLD)
    }

    pub fn status_completed() -> Style {
        Style::default().fg(StatusColors::COMPLETED)
    }

    pub fn status_blocked() -> Style {
        Style::default().fg(StatusColors::BLOCKED).add_modifier(Modifier::BOLD)
    }

    pub fn status_pending() -> Style {
        Style::default().fg(StatusColors::PENDING)
    }

    pub fn status_queued() -> Style {
        Style::default().fg(StatusColors::QUEUED)
    }

    pub fn status_warning() -> Style {
        Style::default().fg(StatusColors::WARNING)
    }

    pub fn context_style(pct: f32) -> Style {
        let color = if pct < 0.6 {
            StatusColors::CONTEXT_LOW
        } else if pct < 0.75 {
            StatusColors::CONTEXT_MED
        } else {
            StatusColors::CONTEXT_HIGH
        };
        Style::default().fg(color)
    }

    pub fn status_merged() -> Style {
        Style::default().fg(StatusColors::MERGED)
    }

    pub fn graph_edge() -> Style {
        Style::default().fg(StatusColors::GRAPH_EDGE)
    }

    pub fn graph_node_active() -> Style {
        Style::default()
            .fg(StatusColors::GRAPH_NODE_ACTIVE)
            .add_modifier(Modifier::BOLD)
    }

    pub fn graph_node_done() -> Style {
        Style::default().fg(StatusColors::GRAPH_NODE_DONE)
    }

    pub fn graph_node_pending() -> Style {
        Style::default().fg(StatusColors::GRAPH_NODE_PENDING)
    }
}
