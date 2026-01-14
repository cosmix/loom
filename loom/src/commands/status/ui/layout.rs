use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Helper for responsive layout calculation
pub struct LayoutHelper {
    pub width: u16,
    pub height: u16,
}

impl LayoutHelper {
    pub fn new(area: Rect) -> Self {
        Self {
            width: area.width,
            height: area.height,
        }
    }

    /// Check if terminal is too narrow for full display
    pub fn is_compact(&self) -> bool {
        self.width < 80
    }

    /// Check if terminal is very narrow (single column mode)
    pub fn is_minimal(&self) -> bool {
        self.width < 40
    }

    /// Split area into header and body
    pub fn header_body(&self, area: Rect, header_height: u16) -> (Rect, Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(header_height), Constraint::Min(0)])
            .split(area);
        (chunks[0], chunks[1])
    }

    /// Create a centered content area with margins
    pub fn centered(&self, area: Rect, margin: u16) -> Rect {
        let horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(margin),
                Constraint::Min(0),
                Constraint::Length(margin),
            ])
            .split(area);
        horizontal[1]
    }

    /// Calculate optimal table column widths based on terminal width
    pub fn table_columns(&self, columns: &[(&str, u16, u16)]) -> Vec<Constraint> {
        // columns: &[(name, min_width, max_width)]
        let _available = self.width.saturating_sub(2); // borders

        columns
            .iter()
            .map(|(_, min, max)| {
                if self.is_compact() {
                    Constraint::Min(*min)
                } else {
                    Constraint::Max(*max)
                }
            })
            .collect()
    }
}

/// Standard layout sections for status display
pub fn main_layout(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Progress bar
            Constraint::Length(1), // Spacer
            Constraint::Min(10),   // Main content (graph + tables)
            Constraint::Length(3), // Footer/activity
        ])
        .split(area)
        .to_vec()
}
