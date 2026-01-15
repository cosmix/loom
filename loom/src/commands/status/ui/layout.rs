use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Minimum minimap width in characters
const MINIMAP_MIN_WIDTH: u16 = 10;

/// Default minimap width percentage
const MINIMAP_WIDTH_PCT: u16 = 20;

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

/// Split an area into graph and minimap sections
///
/// Layout:
///   ┌─────────────────────────────────────────┬──────────┐
///   │                                         │ MINI-MAP │
///   │           MAIN GRAPH                    │          │
///   │         (scrollable)                    │  [    ]  │
///   │                                         │          │
///   └─────────────────────────────────────────┴──────────┘
///
/// Returns (graph_area, minimap_area) where minimap_area is None if not shown
pub fn graph_minimap_split(
    area: Rect,
    show_minimap: bool,
    minimap_width_pct: Option<u16>,
) -> (Rect, Option<Rect>) {
    if !show_minimap {
        return (area, None);
    }

    let pct = minimap_width_pct.unwrap_or(MINIMAP_WIDTH_PCT);
    let minimap_width = (area.width * pct / 100).max(MINIMAP_MIN_WIDTH);
    let graph_width = area.width.saturating_sub(minimap_width);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(graph_width),
            Constraint::Length(minimap_width),
        ])
        .split(area);

    (chunks[0], Some(chunks[1]))
}

/// Calculate optimal graph area height based on content
///
/// Clamps to reasonable bounds: min 6 lines, max 1/3 of screen
pub fn graph_area_height(graph_height: u16, screen_height: u16) -> u16 {
    let min_height = 6;
    let max_height = (screen_height / 3).max(8);
    graph_height.clamp(min_height, max_height)
}

/// Calculate scroll bounds for a viewport
///
/// Returns (max_scroll_x, max_scroll_y)
pub fn scroll_bounds(
    content_width: u16,
    content_height: u16,
    viewport_width: u16,
    viewport_height: u16,
) -> (i32, i32) {
    let max_x = content_width.saturating_sub(viewport_width) as i32;
    let max_y = content_height.saturating_sub(viewport_height) as i32;
    (max_x.max(0), max_y.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_minimap_split_no_minimap() {
        let area = Rect::new(0, 0, 100, 50);
        let (graph, minimap) = graph_minimap_split(area, false, None);

        assert_eq!(graph, area);
        assert!(minimap.is_none());
    }

    #[test]
    fn test_graph_minimap_split_with_minimap() {
        let area = Rect::new(0, 0, 100, 50);
        let (graph, minimap) = graph_minimap_split(area, true, None);

        // Graph should be ~80% width
        assert!(graph.width >= 70 && graph.width <= 90);

        // Minimap should exist and be ~20% width
        let minimap = minimap.expect("minimap should exist");
        assert!(minimap.width >= 10 && minimap.width <= 30);

        // Total should equal original
        assert_eq!(graph.width + minimap.width, area.width);
    }

    #[test]
    fn test_graph_minimap_split_minimum_width() {
        // Small area should still have minimum minimap width
        let area = Rect::new(0, 0, 30, 50);
        let (_graph, minimap) = graph_minimap_split(area, true, None);

        let minimap = minimap.expect("minimap should exist");
        assert!(minimap.width >= MINIMAP_MIN_WIDTH);
    }

    #[test]
    fn test_graph_area_height() {
        // Small content should use minimum
        assert_eq!(graph_area_height(2, 100), 6);

        // Large content should be clamped
        assert_eq!(graph_area_height(100, 30), 10);

        // Normal content should pass through
        assert_eq!(graph_area_height(15, 60), 15);
    }

    #[test]
    fn test_scroll_bounds() {
        // Content smaller than viewport
        let (max_x, max_y) = scroll_bounds(50, 30, 100, 50);
        assert_eq!(max_x, 0);
        assert_eq!(max_y, 0);

        // Content larger than viewport
        let (max_x, max_y) = scroll_bounds(200, 100, 80, 40);
        assert_eq!(max_x, 120);
        assert_eq!(max_y, 60);
    }
}
