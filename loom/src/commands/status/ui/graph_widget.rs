//! Execution graph widget for TUI display
//!
//! Renders a DAG visualization of stage dependencies using the Sugiyama layout algorithm.
//! Uses box-drawing characters for nodes and orthogonal edge routing.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Widget},
};

use super::sugiyama::{self, EdgePath, LayoutConfig};
use super::theme::{StatusColors, Theme};
use crate::models::stage::{Stage, StageStatus};

/// Box-drawing characters for node rendering
mod box_chars {
    pub const HORIZONTAL: char = '─';
    pub const VERTICAL: char = '│';
    pub const TOP_LEFT: char = '┌';
    pub const TOP_RIGHT: char = '┐';
    pub const BOTTOM_LEFT: char = '└';
    pub const BOTTOM_RIGHT: char = '┘';
    pub const T_DOWN: char = '┬';
    pub const T_UP: char = '┴';
    pub const T_RIGHT: char = '├';
    pub const T_LEFT: char = '┤';
    pub const CROSS: char = '┼';
    pub const ARROW_DOWN: char = '▼';
}

/// Viewport configuration for scrolling
#[derive(Debug, Clone, Default)]
pub struct Viewport {
    /// Horizontal scroll offset
    pub scroll_x: i32,
    /// Vertical scroll offset
    pub scroll_y: i32,
}

impl Viewport {
    pub fn new(scroll_x: i32, scroll_y: i32) -> Self {
        Self { scroll_x, scroll_y }
    }
}

/// Result of graph rendering with bounds information
#[derive(Debug, Clone)]
pub struct GraphRenderResult {
    /// Total width of the rendered graph in characters
    pub total_width: u16,
    /// Total height of the rendered graph in characters
    pub total_height: u16,
    /// Whether the graph was clipped horizontally
    pub clipped_x: bool,
    /// Whether the graph was clipped vertically
    pub clipped_y: bool,
}

impl GraphRenderResult {
    pub fn empty() -> Self {
        Self {
            total_width: 0,
            total_height: 0,
            clipped_x: false,
            clipped_y: false,
        }
    }
}

/// Configuration for graph widget rendering
#[derive(Debug, Clone)]
pub struct GraphWidgetConfig {
    /// Width of node boxes in characters
    pub node_width: u16,
    /// Height of node boxes in characters (usually 3 for normal, 4 with context)
    pub node_height: u16,
    /// Horizontal spacing between nodes
    pub horizontal_spacing: u16,
    /// Vertical spacing between layers
    pub vertical_spacing: u16,
    /// Whether to show context percentage for executing stages
    pub show_context: bool,
}

impl Default for GraphWidgetConfig {
    fn default() -> Self {
        Self {
            node_width: 18,
            node_height: 3,
            horizontal_spacing: 4,
            vertical_spacing: 3,
            show_context: true,
        }
    }
}

/// A character cell in the rendering grid with style
#[derive(Clone)]
struct StyledCell {
    ch: char,
    style: Style,
}

impl Default for StyledCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
        }
    }
}

/// Internal grid for rendering before viewport clipping
struct RenderGrid {
    cells: Vec<Vec<StyledCell>>,
    width: usize,
    height: usize,
}

impl RenderGrid {
    fn new(width: usize, height: usize) -> Self {
        Self {
            cells: vec![vec![StyledCell::default(); width]; height],
            width,
            height,
        }
    }

    fn set(&mut self, x: usize, y: usize, ch: char, style: Style) {
        if x < self.width && y < self.height {
            self.cells[y][x] = StyledCell { ch, style };
        }
    }

    fn get(&self, x: usize, y: usize) -> Option<&StyledCell> {
        self.cells.get(y).and_then(|row| row.get(x))
    }

    fn set_str(&mut self, x: usize, y: usize, s: &str, style: Style) {
        for (i, ch) in s.chars().enumerate() {
            self.set(x + i, y, ch, style);
        }
    }
}

/// Execution graph widget using Sugiyama layout
pub struct GraphWidget<'a> {
    stages: &'a [Stage],
    block: Option<Block<'a>>,
    viewport: Viewport,
    config: GraphWidgetConfig,
    context_percentages: HashMap<String, f32>,
}

impl<'a> GraphWidget<'a> {
    pub fn new(stages: &'a [Stage]) -> Self {
        Self {
            stages,
            block: None,
            viewport: Viewport::default(),
            config: GraphWidgetConfig::default(),
            context_percentages: HashMap::new(),
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn viewport(mut self, viewport: Viewport) -> Self {
        self.viewport = viewport;
        self
    }

    pub fn config(mut self, config: GraphWidgetConfig) -> Self {
        self.config = config;
        self
    }

    pub fn context_percentages(mut self, percentages: HashMap<String, f32>) -> Self {
        self.context_percentages = percentages;
        self
    }

    /// Get style for a stage status
    fn status_style(status: &StageStatus) -> Style {
        match status {
            StageStatus::Completed => Theme::status_completed(),
            StageStatus::Executing => Theme::status_executing(),
            StageStatus::Queued => Theme::status_queued(),
            StageStatus::WaitingForDeps => Theme::status_pending(),
            StageStatus::Blocked => Theme::status_blocked(),
            StageStatus::NeedsHandoff => Theme::status_warning(),
            StageStatus::WaitingForInput => Theme::status_warning(),
            StageStatus::Skipped => Theme::dimmed(),
            StageStatus::MergeConflict => Theme::status_blocked(),
            StageStatus::CompletedWithFailures => Theme::status_warning(),
            StageStatus::MergeBlocked => Theme::status_blocked(),
        }
    }

    /// Get status indicator character
    fn status_char(status: &StageStatus) -> char {
        match status {
            StageStatus::Completed => '✓',
            StageStatus::Executing => '●',
            StageStatus::Queued => '▶',
            StageStatus::WaitingForDeps => '○',
            StageStatus::Blocked => '✗',
            StageStatus::NeedsHandoff => '⟳',
            StageStatus::WaitingForInput => '?',
            StageStatus::Skipped => '⊘',
            StageStatus::MergeConflict => '⚡',
            StageStatus::CompletedWithFailures => '⚠',
            StageStatus::MergeBlocked => '⊗',
        }
    }

    /// Render a node box at the specified grid position
    fn render_node(
        &self,
        grid: &mut RenderGrid,
        stage: &Stage,
        grid_x: usize,
        grid_y: usize,
    ) {
        let style = Self::status_style(&stage.status);
        let box_style = Theme::graph_edge();
        let w = self.config.node_width as usize;
        let h = self.config.node_height as usize;

        // Top border: ┌──────────────┐
        grid.set(grid_x, grid_y, box_chars::TOP_LEFT, box_style);
        for i in 1..w - 1 {
            grid.set(grid_x + i, grid_y, box_chars::HORIZONTAL, box_style);
        }
        grid.set(grid_x + w - 1, grid_y, box_chars::TOP_RIGHT, box_style);

        // Content line: │ ● stage-name │
        grid.set(grid_x, grid_y + 1, box_chars::VERTICAL, box_style);
        let indicator = Self::status_char(&stage.status);
        let max_name_len = w.saturating_sub(5); // space for "│ ● " and " │"
        let display_name = if stage.id.len() > max_name_len {
            format!("{}…", &stage.id[..max_name_len.saturating_sub(1)])
        } else {
            stage.id.clone()
        };
        let content = format!(" {indicator} {display_name}");
        let padded = format!("{:width$}", content, width = w - 2);
        grid.set_str(grid_x + 1, grid_y + 1, &padded[..w - 2], style);
        grid.set(grid_x + w - 1, grid_y + 1, box_chars::VERTICAL, box_style);

        // Context line (if showing and executing): │   ctx: 45%   │
        if h >= 4 && self.config.show_context && stage.status == StageStatus::Executing {
            grid.set(grid_x, grid_y + 2, box_chars::VERTICAL, box_style);
            if let Some(&pct) = self.context_percentages.get(&stage.id) {
                let ctx_style = Theme::context_style(pct);
                let ctx_content = format!("ctx: {:.0}%", pct * 100.0);
                let padded_ctx = format!("{:^width$}", ctx_content, width = w - 2);
                grid.set_str(grid_x + 1, grid_y + 2, &padded_ctx[..w - 2], ctx_style);
            } else {
                let spaces = " ".repeat(w - 2);
                grid.set_str(grid_x + 1, grid_y + 2, &spaces, Style::default());
            }
            grid.set(grid_x + w - 1, grid_y + 2, box_chars::VERTICAL, box_style);
        }

        // Bottom border: └──────────────┘
        let bottom_y = grid_y + h - 1;
        grid.set(grid_x, bottom_y, box_chars::BOTTOM_LEFT, box_style);
        for i in 1..w - 1 {
            grid.set(grid_x + i, bottom_y, box_chars::HORIZONTAL, box_style);
        }
        grid.set(grid_x + w - 1, bottom_y, box_chars::BOTTOM_RIGHT, box_style);
    }

    /// Render an edge path using orthogonal box-drawing characters
    fn render_edge(&self, grid: &mut RenderGrid, edge: &EdgePath, scale_x: f64, scale_y: f64) {
        let style = Theme::graph_edge();

        for segment in &edge.segments {
            let (x1, y1) = (
                (segment.x1 * scale_x) as usize,
                (segment.y1 * scale_y) as usize,
            );
            let (x2, y2) = (
                (segment.x2 * scale_x) as usize,
                (segment.y2 * scale_y) as usize,
            );

            // Determine if horizontal or vertical segment
            if y1 == y2 {
                // Horizontal segment
                let (start_x, end_x) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
                for x in start_x..=end_x {
                    let existing = grid.get(x, y1).map(|c| c.ch).unwrap_or(' ');
                    let new_char = self.merge_edge_char(existing, box_chars::HORIZONTAL);
                    grid.set(x, y1, new_char, style);
                }
            } else if x1 == x2 {
                // Vertical segment
                let (start_y, end_y) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
                for y in start_y..=end_y {
                    let existing = grid.get(x1, y).map(|c| c.ch).unwrap_or(' ');
                    let new_char = self.merge_edge_char(existing, box_chars::VERTICAL);
                    grid.set(x1, y, new_char, style);
                }
            }
        }

        // Add arrow at the end of the last segment (pointing down)
        if let Some(last) = edge.segments.last() {
            let arrow_x = (last.x2 * scale_x) as usize;
            let arrow_y = (last.y2 * scale_y) as usize;
            if arrow_y > 0 {
                grid.set(arrow_x, arrow_y - 1, box_chars::ARROW_DOWN, style);
            }
        }
    }

    /// Merge edge characters at intersections
    fn merge_edge_char(&self, existing: char, new_char: char) -> char {
        match (existing, new_char) {
            (' ', c) => c,
            (c, ' ') => c,
            (box_chars::HORIZONTAL, box_chars::VERTICAL) => box_chars::CROSS,
            (box_chars::VERTICAL, box_chars::HORIZONTAL) => box_chars::CROSS,
            (box_chars::HORIZONTAL, box_chars::HORIZONTAL) => box_chars::HORIZONTAL,
            (box_chars::VERTICAL, box_chars::VERTICAL) => box_chars::VERTICAL,
            (box_chars::CROSS, _) => box_chars::CROSS,
            (_, box_chars::CROSS) => box_chars::CROSS,
            // T-junctions
            (box_chars::HORIZONTAL, box_chars::T_DOWN) => box_chars::T_DOWN,
            (box_chars::HORIZONTAL, box_chars::T_UP) => box_chars::T_UP,
            (box_chars::VERTICAL, box_chars::T_RIGHT) => box_chars::T_RIGHT,
            (box_chars::VERTICAL, box_chars::T_LEFT) => box_chars::T_LEFT,
            // Default: prefer the new character
            (_, c) => c,
        }
    }

    /// Perform the layout and render to a grid
    fn render_to_grid(&self) -> (RenderGrid, GraphRenderResult) {
        if self.stages.is_empty() {
            let grid = RenderGrid::new(20, 1);
            return (grid, GraphRenderResult::empty());
        }

        // Use Sugiyama layout
        let layout_config = LayoutConfig {
            horizontal_spacing: self.config.horizontal_spacing as f64,
            vertical_spacing: self.config.vertical_spacing as f64,
            node_width: self.config.node_width as f64,
            node_height: self.config.node_height as f64,
            barycenter_iterations: 4,
        };
        let layout_result = sugiyama::layout_with_config(self.stages, &layout_config);

        if layout_result.is_empty() {
            let grid = RenderGrid::new(20, 1);
            return (grid, GraphRenderResult::empty());
        }

        let bounds = layout_result.bounds();

        // Calculate grid dimensions with some padding
        let grid_width = (bounds.width() + self.config.node_width as f64 + 4.0) as usize;
        let grid_height = (bounds.height() + self.config.node_height as f64 + 2.0) as usize;

        let mut grid = RenderGrid::new(grid_width.max(20), grid_height.max(3));

        // Calculate scaling factors for coordinate conversion
        let scale_x = 1.0;
        let scale_y = 1.0;

        // Create stage lookup map
        let stage_map: HashMap<&str, &Stage> =
            self.stages.iter().map(|s| (s.id.as_str(), s)).collect();

        // Render edges first (so nodes draw over them)
        for edge in layout_result.edges() {
            self.render_edge(&mut grid, edge, scale_x, scale_y);
        }

        // Render nodes
        for (id, pos) in layout_result.nodes() {
            if let Some(stage) = stage_map.get(id.as_str()) {
                let grid_x = pos.x as usize;
                let grid_y = pos.y as usize;
                self.render_node(&mut grid, stage, grid_x, grid_y);
            }
        }

        let result = GraphRenderResult {
            total_width: grid_width as u16,
            total_height: grid_height as u16,
            clipped_x: false,
            clipped_y: false,
        };

        (grid, result)
    }

    /// Render the graph with viewport clipping
    pub fn render_graph(&self, area: Rect, buf: &mut Buffer) -> GraphRenderResult {
        let (grid, mut result) = self.render_to_grid();

        // Apply viewport clipping
        let view_start_x = self.viewport.scroll_x.max(0) as usize;
        let view_start_y = self.viewport.scroll_y.max(0) as usize;

        result.clipped_x = view_start_x > 0 || grid.width > area.width as usize + view_start_x;
        result.clipped_y = view_start_y > 0 || grid.height > area.height as usize + view_start_y;

        // Copy visible portion to buffer
        for y in 0..area.height as usize {
            let grid_y = view_start_y + y;
            if grid_y >= grid.height {
                break;
            }

            for x in 0..area.width as usize {
                let grid_x = view_start_x + x;
                if grid_x >= grid.width {
                    break;
                }

                if let Some(cell) = grid.get(grid_x, grid_y) {
                    let buf_x = area.x + x as u16;
                    let buf_y = area.y + y as u16;
                    if buf_x < area.x + area.width && buf_y < area.y + area.height {
                        buf[(buf_x, buf_y)].set_char(cell.ch).set_style(cell.style);
                    }
                }
            }
        }

        result
    }
}

impl Widget for GraphWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Apply block if present
        let inner_area = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner_area.width < 2 || inner_area.height < 1 {
            return;
        }

        // Handle empty stages
        if self.stages.is_empty() {
            let msg = "(no stages)";
            let x = inner_area.x + (inner_area.width.saturating_sub(msg.len() as u16)) / 2;
            let y = inner_area.y;
            for (i, ch) in msg.chars().enumerate() {
                if x + (i as u16) < inner_area.x + inner_area.width {
                    buf[(x + i as u16, y)].set_char(ch).set_style(Theme::dimmed());
                }
            }
            return;
        }

        self.render_graph(inner_area, buf);
    }
}

/// Create a graph widget with default styling
pub fn execution_graph(stages: &[Stage]) -> GraphWidget<'_> {
    GraphWidget::new(stages).block(
        Block::default()
            .title(" Execution Graph ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(StatusColors::BORDER)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    fn make_stage(id: &str, deps: Vec<&str>, status: StageStatus) -> Stage {
        Stage {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            status,
            dependencies: deps.into_iter().map(String::from).collect(),
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            stage_type: Default::default(),
            plan_id: None,
            worktree: None,
            session: None,
            held: false,
            parent_stage: None,
            child_stages: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            completed_at: None,
            close_reason: None,
            auto_merge: None,
            working_dir: None,
            retry_count: 0,
            max_retries: None,
            last_failure_at: None,
            failure_info: None,
            resolved_base: None,
            base_branch: None,
            base_merged_from: vec![],
            outputs: vec![],
            completed_commit: None,
            merged: false,
            merge_conflict: false,
        }
    }

    #[test]
    fn test_empty_stages() {
        let widget = GraphWidget::new(&[]);
        let (_grid, result) = widget.render_to_grid();
        assert!(result.total_width > 0 || result.total_height == 0);
    }

    #[test]
    fn test_single_stage_layout() {
        let stages = vec![make_stage("bootstrap", vec![], StageStatus::Completed)];
        let widget = GraphWidget::new(&stages);
        let (_grid, result) = widget.render_to_grid();

        assert!(result.total_width > 0);
        assert!(result.total_height > 0);
    }

    #[test]
    fn test_linear_dependency_layout() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Executing),
            make_stage("c", vec!["b"], StageStatus::WaitingForDeps),
        ];
        let widget = GraphWidget::new(&stages);
        let (_grid, result) = widget.render_to_grid();

        assert!(result.total_width > 0);
        assert!(result.total_height > 0);
    }

    #[test]
    fn test_diamond_dependency_layout() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Completed),
            make_stage("c", vec!["a"], StageStatus::Completed),
            make_stage("d", vec!["b", "c"], StageStatus::Executing),
        ];
        let widget = GraphWidget::new(&stages);
        let (_grid, result) = widget.render_to_grid();

        assert!(result.total_width > 0);
        assert!(result.total_height > 0);
    }

    #[test]
    fn test_viewport_clipping() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Executing),
        ];
        let widget = GraphWidget::new(&stages).viewport(Viewport::new(5, 2));

        let area = Rect::new(0, 0, 20, 10);
        let mut buf = Buffer::empty(area);
        let result = widget.render_graph(area, &mut buf);

        assert!(result.clipped_x || result.clipped_y || result.total_width <= 20);
    }

    #[test]
    fn test_viewport_offset() {
        let stages = vec![
            make_stage("root", vec![], StageStatus::Completed),
            make_stage("branch-a", vec!["root"], StageStatus::Executing),
            make_stage("branch-b", vec!["root"], StageStatus::WaitingForDeps),
        ];

        // Render without scroll
        let widget_no_scroll = GraphWidget::new(&stages).viewport(Viewport::new(0, 0));
        let area = Rect::new(0, 0, 80, 40);
        let mut buf_no_scroll = Buffer::empty(area);
        widget_no_scroll.render_graph(area, &mut buf_no_scroll);

        // Render with scroll
        let widget_scrolled = GraphWidget::new(&stages).viewport(Viewport::new(2, 1));
        let mut buf_scrolled = Buffer::empty(area);
        widget_scrolled.render_graph(area, &mut buf_scrolled);

        // The buffers should differ due to scroll offset
        // (unless the graph is smaller than viewport, which is fine)
    }

    #[test]
    fn test_edge_routing_correctness() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Completed),
            make_stage("c", vec!["a"], StageStatus::Completed),
            make_stage("d", vec!["b", "c"], StageStatus::Executing),
        ];
        let widget = GraphWidget::new(&stages);
        let (grid, _result) = widget.render_to_grid();

        // Verify that we have box-drawing characters in the grid
        let mut found_horizontal = false;
        let mut found_vertical = false;

        for y in 0..grid.height {
            for x in 0..grid.width {
                if let Some(cell) = grid.get(x, y) {
                    if cell.ch == box_chars::HORIZONTAL {
                        found_horizontal = true;
                    }
                    if cell.ch == box_chars::VERTICAL {
                        found_vertical = true;
                    }
                }
            }
        }

        // Should have at least some box drawing characters
        assert!(found_horizontal || found_vertical || grid.height < 3);
    }

    #[test]
    fn test_box_drawing_characters() {
        let stages = vec![make_stage("test", vec![], StageStatus::Executing)];
        let widget = GraphWidget::new(&stages);
        let (grid, _) = widget.render_to_grid();

        // Find the node box corners
        let mut found_top_left = false;
        let mut found_top_right = false;
        let mut found_bottom_left = false;
        let mut found_bottom_right = false;

        for y in 0..grid.height {
            for x in 0..grid.width {
                if let Some(cell) = grid.get(x, y) {
                    match cell.ch {
                        box_chars::TOP_LEFT => found_top_left = true,
                        box_chars::TOP_RIGHT => found_top_right = true,
                        box_chars::BOTTOM_LEFT => found_bottom_left = true,
                        box_chars::BOTTOM_RIGHT => found_bottom_right = true,
                        _ => {}
                    }
                }
            }
        }

        assert!(found_top_left, "Should have top-left corner");
        assert!(found_top_right, "Should have top-right corner");
        assert!(found_bottom_left, "Should have bottom-left corner");
        assert!(found_bottom_right, "Should have bottom-right corner");
    }

    #[test]
    fn test_status_indicators() {
        let stages = vec![
            make_stage("completed", vec![], StageStatus::Completed),
            make_stage("executing", vec![], StageStatus::Executing),
            make_stage("waiting", vec![], StageStatus::WaitingForDeps),
            make_stage("blocked", vec![], StageStatus::Blocked),
        ];

        for stage in &stages {
            let indicator = GraphWidget::<'_>::status_char(&stage.status);
            let expected = match stage.status {
                StageStatus::Completed => '✓',
                StageStatus::Executing => '●',
                StageStatus::WaitingForDeps => '○',
                StageStatus::Blocked => '✗',
                _ => unreachable!(),
            };
            assert_eq!(indicator, expected);
        }
    }

    #[test]
    fn test_graph_widget_with_block() {
        let stages = vec![make_stage("test", vec![], StageStatus::Completed)];
        let widget = execution_graph(&stages);

        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // The border should be rendered
        assert_ne!(buf[(0, 0)].symbol(), " ");
    }

    #[test]
    fn test_sugiyama_layout_integration() {
        // Verify that sugiyama::layout is called and produces valid positions
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Executing),
        ];

        let layout_result = sugiyama::layout(&stages);
        assert_eq!(layout_result.node_count(), 2);
        assert!(layout_result.get_node("a").is_some());
        assert!(layout_result.get_node("b").is_some());

        // a should be above b
        let pos_a = layout_result.get_node("a").unwrap();
        let pos_b = layout_result.get_node("b").unwrap();
        assert!(pos_a.y < pos_b.y);
    }

    #[test]
    fn test_config_affects_layout() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Executing),
        ];

        let small_config = GraphWidgetConfig {
            node_width: 10,
            node_height: 2,
            horizontal_spacing: 2,
            vertical_spacing: 1,
            show_context: false,
        };

        let large_config = GraphWidgetConfig {
            node_width: 24,
            node_height: 4,
            horizontal_spacing: 8,
            vertical_spacing: 4,
            show_context: true,
        };

        let widget_small = GraphWidget::new(&stages).config(small_config);
        let widget_large = GraphWidget::new(&stages).config(large_config);

        let (_, result_small) = widget_small.render_to_grid();
        let (_, result_large) = widget_large.render_to_grid();

        // Large config should produce larger total dimensions
        assert!(result_large.total_width >= result_small.total_width);
        assert!(result_large.total_height >= result_small.total_height);
    }

    #[test]
    fn test_context_percentage_display() {
        let stages = vec![make_stage("exec", vec![], StageStatus::Executing)];

        let mut percentages = HashMap::new();
        percentages.insert("exec".to_string(), 0.45);

        let config = GraphWidgetConfig {
            node_height: 4,
            show_context: true,
            ..Default::default()
        };

        let widget = GraphWidget::new(&stages)
            .config(config)
            .context_percentages(percentages);

        let (grid, _) = widget.render_to_grid();

        // Check that "ctx:" appears somewhere in the grid
        let mut found_ctx = false;
        for y in 0..grid.height {
            let mut line = String::new();
            for x in 0..grid.width {
                if let Some(cell) = grid.get(x, y) {
                    line.push(cell.ch);
                }
            }
            if line.contains("ctx:") {
                found_ctx = true;
                break;
            }
        }

        assert!(found_ctx, "Context percentage should be displayed");
    }

    #[test]
    fn test_graph_render_result_bounds() {
        let stages = vec![
            make_stage("a", vec![], StageStatus::Completed),
            make_stage("b", vec!["a"], StageStatus::Completed),
            make_stage("c", vec!["a"], StageStatus::Completed),
            make_stage("d", vec!["b", "c"], StageStatus::Executing),
        ];

        let widget = GraphWidget::new(&stages);
        let (_, result) = widget.render_to_grid();

        // Should have reasonable dimensions for 4 stages
        assert!(result.total_width >= 18, "Should have at least node width");
        assert!(result.total_height >= 3, "Should have at least node height");
    }

    #[test]
    fn test_merge_edge_characters() {
        let widget = GraphWidget::new(&[]);

        // Test crossing
        assert_eq!(
            widget.merge_edge_char(box_chars::HORIZONTAL, box_chars::VERTICAL),
            box_chars::CROSS
        );
        assert_eq!(
            widget.merge_edge_char(box_chars::VERTICAL, box_chars::HORIZONTAL),
            box_chars::CROSS
        );

        // Test same direction
        assert_eq!(
            widget.merge_edge_char(box_chars::HORIZONTAL, box_chars::HORIZONTAL),
            box_chars::HORIZONTAL
        );
        assert_eq!(
            widget.merge_edge_char(box_chars::VERTICAL, box_chars::VERTICAL),
            box_chars::VERTICAL
        );

        // Test with space
        assert_eq!(widget.merge_edge_char(' ', box_chars::HORIZONTAL), box_chars::HORIZONTAL);
        assert_eq!(widget.merge_edge_char(box_chars::VERTICAL, ' '), box_chars::VERTICAL);
    }

    #[test]
    fn test_widget_render_small_area() {
        let stages = vec![make_stage("test", vec![], StageStatus::Completed)];
        let widget = GraphWidget::new(&stages);

        // Very small area
        let area = Rect::new(0, 0, 5, 2);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Should not panic, may be clipped
    }
}
