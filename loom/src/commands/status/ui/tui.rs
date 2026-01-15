//! TUI application for live status dashboard
//!
//! This module provides the ratatui-based terminal UI for displaying
//! live status updates from the loom daemon.
//!
//! Layout (unified design):
//! - Compact header with spinner, title, and inline progress
//! - Execution graph with mini-map (scrollable DAG + overview)
//! - Unified stage table with all columns (status, name, merged, deps, elapsed)
//! - Simplified footer with keybinds and errors
//!
//! Graph area layout:
//!   ┌─────────────────────────────────────────┬──────────┐
//!   │                                         │ MINI-MAP │
//!   │           MAIN GRAPH                    │          │
//!   │         (scrollable)                    │  [    ]  │
//!   │                                         │          │
//!   └─────────────────────────────────────────┴──────────┘

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table, Widget},
    Frame, Terminal,
};

use super::graph_widget::{GraphWidget, GraphWidgetConfig, Viewport};
use super::minimap::MiniMap;
use super::sugiyama::{self, LayoutResult};
use super::theme::{StatusColors, Theme};
use super::widgets::{status_indicator, status_text};
use crate::daemon::{read_message, write_message, Request, Response, StageInfo};
use crate::models::stage::{Stage, StageStatus};

/// Connection timeout for daemon socket
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

/// Poll timeout for event loop (100ms for responsive UI)
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Scroll step for arrow key navigation
const SCROLL_STEP: i32 = 2;

/// Page scroll multiplier (viewport size * this factor)
const PAGE_SCROLL_FACTOR: f64 = 0.8;

/// Mini-map width as percentage of graph area (20%)
const MINIMAP_WIDTH_PCT: u16 = 20;

/// Minimum mini-map width in characters
const MINIMAP_MIN_WIDTH: u16 = 10;

/// Graph state tracking for cached layout and scroll position
#[derive(Default)]
struct GraphState {
    /// Horizontal scroll offset
    scroll_x: i32,
    /// Vertical scroll offset
    scroll_y: i32,
    /// Total graph width in characters
    graph_width: u16,
    /// Total graph height in characters
    graph_height: u16,
    /// Whether minimap is manually toggled off
    minimap_hidden: bool,
    /// Cached layout result (recomputed on stage changes)
    cached_layout: Option<LayoutResult>,
    /// Hash of stage IDs for cache invalidation
    stage_hash: u64,
    /// Last minimap area (for click detection)
    minimap_area: Option<Rect>,
    /// Last graph area (for scroll bounds)
    graph_area: Option<Rect>,
}

impl GraphState {
    /// Check if the graph exceeds the viewport and minimap should be shown
    #[allow(dead_code)]
    fn should_show_minimap(&self, viewport_width: u16, viewport_height: u16) -> bool {
        if self.minimap_hidden {
            return false;
        }
        self.graph_width > viewport_width || self.graph_height > viewport_height
    }

    /// Clamp scroll position to valid bounds
    fn clamp_scroll(&mut self, viewport_width: u16, viewport_height: u16) {
        let max_scroll_x = self.graph_width.saturating_sub(viewport_width) as i32;
        let max_scroll_y = self.graph_height.saturating_sub(viewport_height) as i32;
        self.scroll_x = self.scroll_x.clamp(0, max_scroll_x.max(0));
        self.scroll_y = self.scroll_y.clamp(0, max_scroll_y.max(0));
    }

    /// Scroll by a delta, clamping to bounds
    fn scroll_by(&mut self, dx: i32, dy: i32, viewport_width: u16, viewport_height: u16) {
        self.scroll_x += dx;
        self.scroll_y += dy;
        self.clamp_scroll(viewport_width, viewport_height);
    }

    /// Jump to a specific position (e.g., from minimap click)
    fn scroll_to(&mut self, x: i32, y: i32, viewport_width: u16, viewport_height: u16) {
        self.scroll_x = x;
        self.scroll_y = y;
        self.clamp_scroll(viewport_width, viewport_height);
    }

    /// Compute a hash of stage IDs for cache invalidation
    fn compute_stage_hash(stages: &[UnifiedStage]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        for stage in stages {
            stage.id.hash(&mut hasher);
            std::mem::discriminant(&stage.status).hash(&mut hasher);
        }
        hasher.finish()
    }
}

/// Unified stage entry for the table display
#[derive(Clone)]
struct UnifiedStage {
    id: String,
    status: StageStatus,
    merged: bool,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
    level: usize,
    dependencies: Vec<String>,
}

/// Live status data received from daemon
#[derive(Default)]
struct LiveStatus {
    executing: Vec<StageInfo>,
    pending: Vec<StageInfo>,
    completed: Vec<StageInfo>,
    blocked: Vec<StageInfo>,
}

impl LiveStatus {
    fn total(&self) -> usize {
        self.executing.len() + self.pending.len() + self.completed.len() + self.blocked.len()
    }

    fn progress_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.completed.len() as f64 / total as f64
        }
    }

    /// Compute execution levels for all stages based on dependencies
    fn compute_levels(&self) -> std::collections::HashMap<String, usize> {
        use std::collections::{HashMap, HashSet};

        // Collect all stages into a map
        let all_stages: Vec<&StageInfo> = self
            .executing
            .iter()
            .chain(self.pending.iter())
            .chain(self.completed.iter())
            .chain(self.blocked.iter())
            .collect();

        let stage_map: HashMap<&str, &StageInfo> =
            all_stages.iter().map(|s| (s.id.as_str(), *s)).collect();

        let mut levels: HashMap<String, usize> = HashMap::new();

        fn get_level(
            stage_id: &str,
            stage_map: &HashMap<&str, &StageInfo>,
            levels: &mut HashMap<String, usize>,
            visiting: &mut HashSet<String>,
        ) -> usize {
            if let Some(&level) = levels.get(stage_id) {
                return level;
            }

            // Cycle detection - treat as level 0 to avoid infinite recursion
            if visiting.contains(stage_id) {
                return 0;
            }
            visiting.insert(stage_id.to_string());

            let stage = match stage_map.get(stage_id) {
                Some(s) => s,
                None => return 0,
            };

            let level = if stage.dependencies.is_empty() {
                0
            } else {
                stage
                    .dependencies
                    .iter()
                    .map(|dep| get_level(dep, stage_map, levels, visiting))
                    .max()
                    .unwrap_or(0)
                    + 1
            };

            visiting.remove(stage_id);
            levels.insert(stage_id.to_string(), level);
            level
        }

        for stage in &all_stages {
            let mut visiting = HashSet::new();
            get_level(&stage.id, &stage_map, &mut levels, &mut visiting);
        }

        levels
    }

    /// Build unified list of all stages for table display, sorted by execution order
    fn unified_stages(&self) -> Vec<UnifiedStage> {
        use std::collections::HashSet;

        let levels = self.compute_levels();
        let mut stages = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        // Helper to convert StageInfo to UnifiedStage with level
        let to_unified =
            |stage: &StageInfo, levels: &std::collections::HashMap<String, usize>| UnifiedStage {
                id: stage.id.clone(),
                status: stage.status.clone(),
                merged: stage.merged,
                started_at: Some(stage.started_at),
                completed_at: stage.completed_at,
                level: levels.get(&stage.id).copied().unwrap_or(0),
                dependencies: stage.dependencies.clone(),
            };

        // Add all stages from each category
        for stage in &self.executing {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        for stage in &self.completed {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        for stage in &self.pending {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        for stage in &self.blocked {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        // Sort by level (execution order), then by id for consistency
        stages.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.id.cmp(&b.id)));

        stages
    }
}

/// TUI application state
pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: Arc<AtomicBool>,
    status: LiveStatus,
    spinner_frame: usize,
    last_error: Option<String>,
    /// Graph scrolling and caching state
    graph_state: GraphState,
    /// Mouse support enabled
    mouse_enabled: bool,
}

impl TuiApp {
    /// Create a new TUI application
    pub fn new() -> Result<Self> {
        // Set up terminal
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;

        // Enable mouse capture for minimap interaction
        let mouse_enabled = crossterm::execute!(stdout, crossterm::event::EnableMouseCapture).is_ok();

        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).context("Failed to create terminal")?;

        Ok(Self {
            terminal,
            running: Arc::new(AtomicBool::new(true)),
            status: LiveStatus::default(),
            spinner_frame: 0,
            last_error: None,
            graph_state: GraphState::default(),
            mouse_enabled,
        })
    }

    /// Run the TUI event loop
    pub fn run(&mut self, work_path: &Path) -> Result<()> {
        let socket_path = work_path.join("orchestrator.sock");
        let mut stream = self.connect(&socket_path)?;
        self.subscribe(&mut stream)?;

        // Set stream to non-blocking for event loop
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .ok();

        while self.running.load(Ordering::SeqCst) {
            // Handle daemon messages (non-blocking)
            let msg_result: Result<Response> = read_message(&mut stream);
            if let Ok(response) = msg_result {
                self.handle_response(response);
            }

            // Handle input events (keyboard and mouse)
            if event::poll(POLL_TIMEOUT)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        self.handle_key_event(key.code, key.modifiers);
                    }
                    Event::Mouse(mouse) => {
                        self.handle_mouse_event(mouse);
                    }
                    _ => {}
                }
            }

            // Update spinner
            self.spinner_frame = (self.spinner_frame + 1) % 10;

            // Render
            self.render()?;
        }

        // Cleanup: unsubscribe from daemon
        let _ = write_message(&mut stream, &Request::Unsubscribe);

        Ok(())
    }

    /// Handle keyboard events for navigation and control
    fn handle_key_event(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        // Get viewport dimensions for scroll bounds
        let (viewport_w, viewport_h) = self
            .graph_state
            .graph_area
            .map(|r| (r.width, r.height))
            .unwrap_or((80, 20));

        match code {
            // Quit
            KeyCode::Char('q') | KeyCode::Esc => {
                self.running.store(false, Ordering::SeqCst);
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.running.store(false, Ordering::SeqCst);
            }

            // Arrow key navigation
            KeyCode::Up => {
                self.graph_state.scroll_by(0, -SCROLL_STEP, viewport_w, viewport_h);
            }
            KeyCode::Down => {
                self.graph_state.scroll_by(0, SCROLL_STEP, viewport_w, viewport_h);
            }
            KeyCode::Left => {
                self.graph_state.scroll_by(-SCROLL_STEP, 0, viewport_w, viewport_h);
            }
            KeyCode::Right => {
                self.graph_state.scroll_by(SCROLL_STEP, 0, viewport_w, viewport_h);
            }

            // Home/End: jump to start/end
            KeyCode::Home => {
                self.graph_state.scroll_to(0, 0, viewport_w, viewport_h);
            }
            KeyCode::End => {
                let max_x = self.graph_state.graph_width.saturating_sub(viewport_w) as i32;
                let max_y = self.graph_state.graph_height.saturating_sub(viewport_h) as i32;
                self.graph_state.scroll_to(max_x, max_y, viewport_w, viewport_h);
            }

            // Page Up/Down: scroll by page
            KeyCode::PageUp => {
                let page_step = (viewport_h as f64 * PAGE_SCROLL_FACTOR) as i32;
                self.graph_state.scroll_by(0, -page_step, viewport_w, viewport_h);
            }
            KeyCode::PageDown => {
                let page_step = (viewport_h as f64 * PAGE_SCROLL_FACTOR) as i32;
                self.graph_state.scroll_by(0, page_step, viewport_w, viewport_h);
            }

            // Toggle minimap
            KeyCode::Char('m') => {
                self.graph_state.minimap_hidden = !self.graph_state.minimap_hidden;
            }

            _ => {}
        }
    }

    /// Handle mouse events for minimap interaction and scrolling
    fn handle_mouse_event(&mut self, mouse: crossterm::event::MouseEvent) {
        let (viewport_w, viewport_h) = self
            .graph_state
            .graph_area
            .map(|r| (r.width, r.height))
            .unwrap_or((80, 20));

        match mouse.kind {
            // Click in minimap to jump to location
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(minimap_area) = self.graph_state.minimap_area {
                    let x = mouse.column;
                    let y = mouse.row;

                    // Check if click is within minimap bounds
                    if x >= minimap_area.x
                        && x < minimap_area.x + minimap_area.width
                        && y >= minimap_area.y
                        && y < minimap_area.y + minimap_area.height
                    {
                        // Convert minimap click to graph scroll position
                        if let Some(ref layout) = self.graph_state.cached_layout {
                            let minimap = MiniMap::new(layout);
                            let (scroll_x, scroll_y) =
                                minimap.point_to_scroll(x, y, minimap_area);

                            // Center the viewport on the clicked position
                            let center_x = scroll_x as i32 - (viewport_w / 2) as i32;
                            let center_y = scroll_y as i32 - (viewport_h / 2) as i32;
                            self.graph_state.scroll_to(center_x, center_y, viewport_w, viewport_h);
                        }
                    }
                }
            }

            // Scroll wheel to pan graph
            MouseEventKind::ScrollUp => {
                self.graph_state.scroll_by(0, -SCROLL_STEP * 2, viewport_w, viewport_h);
            }
            MouseEventKind::ScrollDown => {
                self.graph_state.scroll_by(0, SCROLL_STEP * 2, viewport_w, viewport_h);
            }

            _ => {}
        }
    }

    /// Connect to daemon socket
    fn connect(&self, socket_path: &Path) -> Result<UnixStream> {
        let mut stream =
            UnixStream::connect(socket_path).context("Failed to connect to daemon socket")?;

        stream
            .set_read_timeout(Some(SOCKET_TIMEOUT))
            .context("Failed to set read timeout")?;
        stream
            .set_write_timeout(Some(SOCKET_TIMEOUT))
            .context("Failed to set write timeout")?;

        // Ping to verify daemon is responsive
        write_message(&mut stream, &Request::Ping).context("Failed to send Ping")?;

        let response: Response =
            read_message(&mut stream).context("Failed to read Ping response")?;

        match response {
            Response::Pong => {}
            Response::Error { message } => {
                anyhow::bail!("Daemon returned error: {message}");
            }
            _ => {
                anyhow::bail!("Unexpected response from daemon");
            }
        }

        Ok(stream)
    }

    /// Subscribe to status updates
    fn subscribe(&self, stream: &mut UnixStream) -> Result<()> {
        write_message(stream, &Request::SubscribeStatus)
            .context("Failed to send SubscribeStatus")?;

        let response: Response =
            read_message(stream).context("Failed to read subscription response")?;

        match response {
            Response::Ok => Ok(()),
            Response::Error { message } => {
                anyhow::bail!("Subscription failed: {message}");
            }
            _ => {
                anyhow::bail!("Unexpected subscription response");
            }
        }
    }

    /// Handle a response from the daemon
    fn handle_response(&mut self, response: Response) {
        match response {
            Response::StatusUpdate {
                stages_executing,
                stages_pending,
                stages_completed,
                stages_blocked,
            } => {
                self.status = LiveStatus {
                    executing: stages_executing,
                    pending: stages_pending,
                    completed: stages_completed,
                    blocked: stages_blocked,
                };
                self.last_error = None;
            }
            Response::Error { message } => {
                self.last_error = Some(message);
            }
            _ => {}
        }
    }

    /// Render the UI
    fn render(&mut self) -> Result<()> {
        // Extract all data we need before entering the closure
        let spinner = self.spinner_char();
        let status = &self.status;
        let last_error = self.last_error.clone();

        // Pre-compute values for rendering
        let pct = status.progress_pct();
        let total = status.total();
        let completed_count = status.completed.len();

        // Clone the data we need for rendering
        let unified_stages = status.unified_stages();

        // Convert UnifiedStages to Stages for the graph widget
        let stages_for_graph: Vec<Stage> = unified_stages
            .iter()
            .map(unified_stage_to_stage)
            .collect();

        // Check if layout needs recomputation (stage hash changed)
        let new_hash = GraphState::compute_stage_hash(&unified_stages);
        if self.graph_state.stage_hash != new_hash || self.graph_state.cached_layout.is_none() {
            // Recompute layout
            let layout = sugiyama::layout(&stages_for_graph);
            let bounds = layout.bounds();
            self.graph_state.graph_width = bounds.width() as u16 + 20; // padding
            self.graph_state.graph_height = bounds.height() as u16 + 4; // padding
            self.graph_state.cached_layout = Some(layout);
            self.graph_state.stage_hash = new_hash;
        }

        // Build status map for minimap coloring
        let status_map: HashMap<String, StageStatus> = unified_stages
            .iter()
            .map(|s| (s.id.clone(), s.status.clone()))
            .collect();

        // Extract state for the closure
        let scroll_x = self.graph_state.scroll_x;
        let scroll_y = self.graph_state.scroll_y;
        let graph_width = self.graph_state.graph_width;
        let graph_height = self.graph_state.graph_height;
        let minimap_hidden = self.graph_state.minimap_hidden;
        let cached_layout = self.graph_state.cached_layout.clone();

        // Track areas for event handling
        let mut graph_area_out = None;
        let mut minimap_area_out = None;

        self.terminal.draw(|frame| {
            let area = frame.area();

            // Dynamic graph height based on content, clamped to reasonable bounds
            let graph_area_height = (graph_height).clamp(6, (area.height / 3).max(8));

            // Layout with breathing room:
            // - Compact header (1 line)
            // - Spacer (1 line)
            // - Execution graph with minimap (dynamic, min 6 lines)
            // - Spacer (1 line)
            // - Unified stage table (remaining space)
            // - Footer (2 lines for keybinds)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),                 // Compact header with inline progress
                    Constraint::Length(1),                 // Spacer
                    Constraint::Length(graph_area_height), // Execution graph + minimap
                    Constraint::Length(1),                 // Spacer
                    Constraint::Min(6),                    // Unified stage table
                    Constraint::Length(2),                 // Footer with keybinds
                ])
                .split(area);

            render_compact_header(frame, chunks[0], spinner, pct, completed_count, total);
            // chunks[1] is spacer - left empty

            // Render graph area with optional minimap
            let (graph_rect, minimap_rect) = render_graph_with_minimap(
                frame,
                chunks[2],
                &stages_for_graph,
                &status_map,
                cached_layout.as_ref(),
                scroll_x,
                scroll_y,
                graph_width,
                graph_height,
                minimap_hidden,
            );
            graph_area_out = Some(graph_rect);
            minimap_area_out = minimap_rect;

            // chunks[3] is spacer - left empty
            render_unified_table(frame, chunks[4], &unified_stages);
            render_compact_footer(frame, chunks[5], &last_error);
        })?;

        // Update stored areas for event handling
        self.graph_state.graph_area = graph_area_out;
        self.graph_state.minimap_area = minimap_area_out;

        Ok(())
    }

    /// Get spinner character for current frame
    fn spinner_char(&self) -> char {
        const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        SPINNER[self.spinner_frame % SPINNER.len()]
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        // Restore terminal state
        let _ = disable_raw_mode();
        // Disable mouse capture if it was enabled
        if self.mouse_enabled {
            let _ = crossterm::execute!(
                self.terminal.backend_mut(),
                crossterm::event::DisableMouseCapture
            );
        }
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}

/// Render compact header with inline progress
fn render_compact_header(
    frame: &mut Frame,
    area: Rect,
    spinner: char,
    pct: f64,
    completed_count: usize,
    total: usize,
) {
    let progress_str = format!("{completed_count}/{total} ({:.0}%)", pct * 100.0);

    let header_line = Line::from(vec![
        Span::styled(format!("{spinner} "), Theme::header()),
        Span::styled("Loom", Theme::header()),
        Span::raw(" │ "),
        Span::styled(progress_str, Style::default().fg(StatusColors::COMPLETED)),
        Span::raw(" "),
        Span::styled(progress_bar_compact(pct, 20), Theme::status_completed()),
    ]);

    let header = Paragraph::new(header_line);
    frame.render_widget(header, area);
}

/// Create a compact progress bar string
fn progress_bar_compact(pct: f64, width: usize) -> String {
    let filled = (pct * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

/// Convert UnifiedStage to Stage for graph widget compatibility
fn unified_stage_to_stage(us: &UnifiedStage) -> Stage {
    use chrono::Utc;

    Stage {
        id: us.id.clone(),
        name: us.id.clone(),
        description: None,
        status: us.status.clone(),
        dependencies: us.dependencies.clone(),
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
        created_at: us.started_at.unwrap_or_else(Utc::now),
        updated_at: Utc::now(),
        completed_at: us.completed_at,
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
        merged: us.merged,
        merge_conflict: false,
    }
}

/// Render execution graph with optional mini-map overlay
///
/// Layout:
///   ┌─────────────────────────────────────────┬──────────┐
///   │                                         │ MINI-MAP │
///   │           MAIN GRAPH                    │          │
///   │         (scrollable)                    │  [    ]  │
///   │                                         │          │
///   └─────────────────────────────────────────┴──────────┘
///
/// Returns (graph_area, minimap_area) for event handling
#[allow(clippy::too_many_arguments)]
fn render_graph_with_minimap(
    frame: &mut Frame,
    area: Rect,
    stages: &[Stage],
    status_map: &HashMap<String, StageStatus>,
    cached_layout: Option<&LayoutResult>,
    scroll_x: i32,
    scroll_y: i32,
    graph_width: u16,
    graph_height: u16,
    minimap_hidden: bool,
) -> (Rect, Option<Rect>) {
    if stages.is_empty() {
        let empty = Paragraph::new(Span::styled("No stages", Theme::dimmed())).block(
            Block::default()
                .title(" Execution Graph ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(StatusColors::BORDER)),
        );
        frame.render_widget(empty, area);
        return (area, None);
    }

    // Determine if minimap should be shown (auto-hide for small graphs)
    let viewport_width = area.width.saturating_sub(2); // account for borders
    let viewport_height = area.height.saturating_sub(2);
    let show_minimap = !minimap_hidden
        && (graph_width > viewport_width || graph_height > viewport_height);

    // Calculate layout split
    let (graph_area, minimap_area) = if show_minimap {
        let minimap_width = (area.width * MINIMAP_WIDTH_PCT / 100).max(MINIMAP_MIN_WIDTH);
        let graph_width_area = area.width.saturating_sub(minimap_width);

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(graph_width_area), // Main graph (80%)
                Constraint::Length(minimap_width),    // Minimap (20%)
            ])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    // Render main graph with viewport
    let graph_block = Block::default()
        .title(" Execution Graph ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    let inner_graph_area = graph_block.inner(graph_area);
    frame.render_widget(graph_block.clone(), graph_area);

    // Create and render GraphWidget with viewport offset
    let viewport = Viewport::new(scroll_x, scroll_y);
    let graph_widget = GraphWidget::new(stages)
        .viewport(viewport)
        .config(GraphWidgetConfig::default());

    // Render graph into the inner area
    let buf = frame.buffer_mut();
    graph_widget.render_graph(inner_graph_area, buf);

    // Render minimap if shown
    if let (Some(minimap_rect), Some(layout)) = (minimap_area, cached_layout) {
        let minimap_block = Block::default()
            .title(" Map ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(StatusColors::BORDER));

        let inner_minimap_area = minimap_block.inner(minimap_rect);
        frame.render_widget(minimap_block, minimap_rect);

        // Create viewport rectangle for overlay (representing visible area)
        let viewport_rect = Rect::new(
            scroll_x.max(0) as u16,
            scroll_y.max(0) as u16,
            inner_graph_area.width,
            inner_graph_area.height,
        );

        // Create and render minimap
        let mut minimap = MiniMap::new(layout);
        minimap.set_viewport(viewport_rect);
        minimap.set_statuses(status_map.clone());

        let buf = frame.buffer_mut();
        minimap.render(inner_minimap_area, buf);

        (inner_graph_area, Some(inner_minimap_area))
    } else {
        (inner_graph_area, None)
    }
}

/// Render unified stage table with all columns
fn render_unified_table(frame: &mut Frame, area: Rect, stages: &[UnifiedStage]) {
    let block = Block::default()
        .title(format!(" Stages ({}) ", stages.len()))
        .title_style(Theme::header())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    if stages.is_empty() {
        let empty = Paragraph::new("No stages")
            .style(Theme::dimmed())
            .block(block);
        frame.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec!["", "Lvl", "ID", "Status", "Merged", "Elapsed"])
        .style(Theme::header())
        .bottom_margin(1);

    let rows: Vec<Row> = stages
        .iter()
        .map(|stage| {
            let icon = status_indicator(&stage.status);
            let status_str = status_text(&stage.status);
            let merged_str = if stage.merged { "✓" } else { "○" };

            let level_str = stage.level.to_string();

            // Show elapsed time: live for executing, final duration for completed
            let elapsed_str = match (&stage.status, stage.started_at, stage.completed_at) {
                // Executing: show live elapsed time
                (StageStatus::Executing, Some(start), _) => {
                    let elapsed = chrono::Utc::now()
                        .signed_duration_since(start)
                        .num_seconds();
                    format_elapsed(elapsed)
                }
                // Completed/blocked/etc with completed_at: show final duration
                (_, Some(start), Some(end)) => {
                    let elapsed = end.signed_duration_since(start).num_seconds();
                    format_elapsed(elapsed)
                }
                // No timing info available
                _ => "-".to_string(),
            };

            let style = match stage.status {
                StageStatus::Executing => Theme::status_executing(),
                StageStatus::Completed => Theme::status_completed(),
                StageStatus::Blocked | StageStatus::MergeConflict | StageStatus::MergeBlocked => {
                    Theme::status_blocked()
                }
                StageStatus::NeedsHandoff
                | StageStatus::WaitingForInput
                | StageStatus::CompletedWithFailures => Theme::status_warning(),
                StageStatus::Queued => Theme::status_queued(),
                _ => Theme::dimmed(),
            };

            Row::new(vec![
                icon.content.to_string(),
                level_str,
                stage.id.clone(),
                status_str.to_string(),
                merged_str.to_string(),
                elapsed_str,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),  // Icon
        Constraint::Length(3),  // Level
        Constraint::Min(20),    // ID
        Constraint::Length(10), // Status
        Constraint::Length(6),  // Merged
        Constraint::Length(8),  // Elapsed
    ];

    let table = Table::new(rows, widths).block(block).header(header);
    frame.render_widget(table, area);
}

/// Render compact footer with keybinds
fn render_compact_footer(frame: &mut Frame, area: Rect, last_error: &Option<String>) {
    let lines = if let Some(ref err) = last_error {
        vec![
            Line::from(vec![
                Span::styled("Error: ", Style::default().fg(StatusColors::BLOCKED)),
                Span::styled(err.as_str(), Style::default().fg(StatusColors::BLOCKED)),
            ]),
            Line::default(),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" quit │ "),
                Span::styled("↑↓←→", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" scroll │ "),
                Span::styled("PgUp/PgDn", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" page │ "),
                Span::styled("Home/End", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" jump"),
            ]),
            Line::from(vec![
                Span::styled("m", Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" toggle minimap │ "),
                Span::styled("Daemon runs in background", Theme::dimmed()),
            ]),
        ]
    };

    let footer = Paragraph::new(lines);
    frame.render_widget(footer, area);
}

/// Format elapsed time in human-readable format
fn format_elapsed(seconds: i64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m{}s", seconds / 60, seconds % 60)
    } else {
        format!("{}h{}m", seconds / 3600, (seconds % 3600) / 60)
    }
}

/// Entry point for TUI live mode
pub fn run_tui(work_path: &Path) -> Result<()> {
    let mut app = TuiApp::new()?;
    app.run(work_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_unified_stage(id: &str, deps: Vec<&str>, status: StageStatus, level: usize) -> UnifiedStage {
        UnifiedStage {
            id: id.to_string(),
            status,
            merged: false,
            started_at: None,
            completed_at: None,
            level,
            dependencies: deps.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn test_graph_state_default() {
        let state = GraphState::default();
        assert_eq!(state.scroll_x, 0);
        assert_eq!(state.scroll_y, 0);
        assert_eq!(state.graph_width, 0);
        assert_eq!(state.graph_height, 0);
        assert!(!state.minimap_hidden);
        assert!(state.cached_layout.is_none());
    }

    #[test]
    fn test_graph_state_scroll_clamp() {
        let mut state = GraphState {
            graph_width: 100,
            graph_height: 50,
            ..Default::default()
        };

        // Test clamping to bounds
        state.scroll_x = -10;
        state.scroll_y = -10;
        state.clamp_scroll(50, 25);
        assert_eq!(state.scroll_x, 0);
        assert_eq!(state.scroll_y, 0);

        // Test clamping at max
        state.scroll_x = 100;
        state.scroll_y = 100;
        state.clamp_scroll(50, 25);
        assert_eq!(state.scroll_x, 50); // 100 - 50
        assert_eq!(state.scroll_y, 25); // 50 - 25
    }

    #[test]
    fn test_graph_state_scroll_by() {
        let mut state = GraphState {
            graph_width: 100,
            graph_height: 50,
            scroll_x: 10,
            scroll_y: 10,
            ..Default::default()
        };

        state.scroll_by(5, 3, 50, 25);
        assert_eq!(state.scroll_x, 15);
        assert_eq!(state.scroll_y, 13);

        // Scroll beyond bounds should clamp
        state.scroll_by(100, 100, 50, 25);
        assert_eq!(state.scroll_x, 50); // clamped to max
        assert_eq!(state.scroll_y, 25);

        // Scroll negative beyond bounds
        state.scroll_by(-200, -200, 50, 25);
        assert_eq!(state.scroll_x, 0);
        assert_eq!(state.scroll_y, 0);
    }

    #[test]
    fn test_graph_state_scroll_to() {
        let mut state = GraphState {
            graph_width: 100,
            graph_height: 50,
            ..Default::default()
        };

        state.scroll_to(25, 15, 50, 25);
        assert_eq!(state.scroll_x, 25);
        assert_eq!(state.scroll_y, 15);

        // Scroll to negative should clamp to 0
        state.scroll_to(-10, -10, 50, 25);
        assert_eq!(state.scroll_x, 0);
        assert_eq!(state.scroll_y, 0);
    }

    #[test]
    fn test_graph_state_should_show_minimap() {
        let state = GraphState {
            graph_width: 100,
            graph_height: 50,
            minimap_hidden: false,
            ..Default::default()
        };

        // Should show when content exceeds viewport
        assert!(state.should_show_minimap(80, 40));

        // Should not show when content fits
        assert!(!state.should_show_minimap(120, 60));

        // Should not show when manually hidden
        let hidden_state = GraphState {
            graph_width: 100,
            graph_height: 50,
            minimap_hidden: true,
            ..Default::default()
        };
        assert!(!hidden_state.should_show_minimap(80, 40));
    }

    #[test]
    fn test_compute_stage_hash_changes() {
        let stages1 = vec![
            make_unified_stage("a", vec![], StageStatus::Completed, 0),
            make_unified_stage("b", vec!["a"], StageStatus::Executing, 1),
        ];
        let stages2 = vec![
            make_unified_stage("a", vec![], StageStatus::Completed, 0),
            make_unified_stage("b", vec!["a"], StageStatus::Completed, 1), // status changed
        ];
        let stages3 = vec![
            make_unified_stage("a", vec![], StageStatus::Completed, 0),
            make_unified_stage("c", vec!["a"], StageStatus::Executing, 1), // id changed
        ];

        let hash1 = GraphState::compute_stage_hash(&stages1);
        let hash2 = GraphState::compute_stage_hash(&stages2);
        let hash3 = GraphState::compute_stage_hash(&stages3);

        // Same stages, same hash
        let hash1_again = GraphState::compute_stage_hash(&stages1);
        assert_eq!(hash1, hash1_again);

        // Different status should produce different hash
        assert_ne!(hash1, hash2);

        // Different id should produce different hash
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_unified_stage_to_stage_conversion() {
        let unified = UnifiedStage {
            id: "test-stage".to_string(),
            status: StageStatus::Executing,
            merged: true,
            started_at: Some(chrono::Utc::now()),
            completed_at: None,
            level: 2,
            dependencies: vec!["dep1".to_string(), "dep2".to_string()],
        };

        let stage = unified_stage_to_stage(&unified);

        assert_eq!(stage.id, "test-stage");
        assert_eq!(stage.status, StageStatus::Executing);
        assert!(stage.merged);
        assert_eq!(stage.dependencies, vec!["dep1".to_string(), "dep2".to_string()]);
    }

    #[test]
    fn test_live_status_progress() {
        let mut status = LiveStatus::default();
        assert_eq!(status.total(), 0);
        assert_eq!(status.progress_pct(), 0.0);

        status.pending = vec![
            StageInfo {
                id: "a".to_string(),
                name: "Stage A".to_string(),
                session_pid: None,
                started_at: chrono::Utc::now(),
                completed_at: None,
                worktree_status: None,
                status: StageStatus::WaitingForDeps,
                merged: false,
                dependencies: vec![],
            },
        ];
        status.completed = vec![
            StageInfo {
                id: "b".to_string(),
                name: "Stage B".to_string(),
                session_pid: None,
                started_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                worktree_status: None,
                status: StageStatus::Completed,
                merged: true,
                dependencies: vec![],
            },
        ];

        assert_eq!(status.total(), 2);
        assert_eq!(status.progress_pct(), 0.5); // 1/2 completed
    }

    #[test]
    fn test_live_status_compute_levels() {
        let status = LiveStatus {
            executing: vec![],
            pending: vec![
                StageInfo {
                    id: "a".to_string(),
                    name: "A".to_string(),
                    session_pid: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                    worktree_status: None,
                    status: StageStatus::WaitingForDeps,
                    merged: false,
                    dependencies: vec![],
                },
                StageInfo {
                    id: "b".to_string(),
                    name: "B".to_string(),
                    session_pid: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                    worktree_status: None,
                    status: StageStatus::WaitingForDeps,
                    merged: false,
                    dependencies: vec!["a".to_string()],
                },
                StageInfo {
                    id: "c".to_string(),
                    name: "C".to_string(),
                    session_pid: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                    worktree_status: None,
                    status: StageStatus::WaitingForDeps,
                    merged: false,
                    dependencies: vec!["a".to_string(), "b".to_string()],
                },
            ],
            completed: vec![],
            blocked: vec![],
        };

        let levels = status.compute_levels();

        assert_eq!(levels.get("a"), Some(&0)); // no deps
        assert_eq!(levels.get("b"), Some(&1)); // depends on a
        assert_eq!(levels.get("c"), Some(&2)); // depends on a and b
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(30), "30s");
        assert_eq!(format_elapsed(90), "1m30s");
        assert_eq!(format_elapsed(3661), "1h1m");
    }
}
