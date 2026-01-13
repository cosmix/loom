//! TUI application for live status dashboard
//!
//! This module provides the ratatui-based terminal UI for displaying
//! live status updates from the loom daemon.
//!
//! Layout (unified design):
//! - Compact header with spinner, title, and inline progress
//! - Execution graph (ASCII DAG showing dependency flow)
//! - Unified stage table with all columns (status, name, merged, deps, elapsed)
//! - Simplified footer with keybinds and errors

use std::io::{self, Stdout};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Table},
    Frame, Terminal,
};

use super::theme::{StatusColors, Theme};
use super::widgets::{render_execution_graph, status_indicator, status_text};
use crate::daemon::{read_message, write_message, Request, Response, StageInfo};
use crate::models::stage::StageStatus;

/// Connection timeout for daemon socket
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

/// Poll timeout for event loop (100ms for responsive UI)
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Unified stage entry for the table display
#[derive(Clone)]
struct UnifiedStage {
    id: String,
    status: StageStatus,
    merged: bool,
    dependencies: Vec<String>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Live status data received from daemon
#[derive(Default)]
struct LiveStatus {
    executing: Vec<StageInfo>,
    pending: Vec<String>,
    completed: Vec<String>,
    blocked: Vec<String>,
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

    /// Build unified list of all stages for table display
    fn unified_stages(&self) -> Vec<UnifiedStage> {
        let mut stages = Vec::new();

        // Add executing stages (have full info)
        for stage in &self.executing {
            stages.push(UnifiedStage {
                id: stage.id.clone(),
                status: stage.status.clone(),
                merged: stage.merged,
                dependencies: stage.dependencies.clone(),
                started_at: Some(stage.started_at),
            });
        }

        // Add completed stages (minimal info)
        for id in &self.completed {
            if !stages.iter().any(|s| &s.id == id) {
                stages.push(UnifiedStage {
                    id: id.clone(),
                    status: StageStatus::Completed,
                    merged: true,
                    dependencies: vec![],
                    started_at: None,
                });
            }
        }

        // Add pending stages
        for id in &self.pending {
            if !stages.iter().any(|s| &s.id == id) {
                stages.push(UnifiedStage {
                    id: id.clone(),
                    status: StageStatus::WaitingForDeps,
                    merged: false,
                    dependencies: vec![],
                    started_at: None,
                });
            }
        }

        // Add blocked stages
        for id in &self.blocked {
            if !stages.iter().any(|s| &s.id == id) {
                stages.push(UnifiedStage {
                    id: id.clone(),
                    status: StageStatus::Blocked,
                    merged: false,
                    dependencies: vec![],
                    started_at: None,
                });
            }
        }

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
}

impl TuiApp {
    /// Create a new TUI application
    pub fn new() -> Result<Self> {
        // Set up terminal
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend).context("Failed to create terminal")?;

        Ok(Self {
            terminal,
            running: Arc::new(AtomicBool::new(true)),
            status: LiveStatus::default(),
            spinner_frame: 0,
            last_error: None,
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

            // Handle keyboard input
            if event::poll(POLL_TIMEOUT)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                self.running.store(false, Ordering::SeqCst);
                            }
                            _ => {}
                        }
                    }
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
        let executing = status.executing.clone();
        let pending = status.pending.clone();
        let completed = status.completed.clone();
        let blocked = status.blocked.clone();
        let unified_stages = status.unified_stages();

        self.terminal.draw(|frame| {
            let area = frame.area();

            // New unified layout:
            // - Compact header (1 line with inline progress)
            // - Execution graph (3 lines)
            // - Unified stage table (remaining space)
            // - Footer (1 line)
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Compact header with inline progress
                    Constraint::Length(3), // Execution graph
                    Constraint::Min(8),    // Unified stage table
                    Constraint::Length(1), // Footer
                ])
                .split(area);

            render_compact_header(frame, chunks[0], spinner, pct, completed_count, total);
            render_graph(frame, chunks[1], &executing, &pending, &completed, &blocked);
            render_unified_table(frame, chunks[2], &unified_stages);
            render_compact_footer(frame, chunks[3], &last_error);
        })?;

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

/// Render execution graph
fn render_graph(
    frame: &mut Frame,
    area: Rect,
    executing: &[StageInfo],
    pending: &[String],
    completed: &[String],
    blocked: &[String],
) {
    let graph_lines = render_execution_graph(executing, pending, completed, blocked);

    let block = Block::default()
        .title(" Execution Graph ")
        .title_style(Theme::dimmed())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    let paragraph = Paragraph::new(graph_lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render unified stage table with all columns
fn render_unified_table(frame: &mut Frame, area: Rect, stages: &[UnifiedStage]) {
    let block = Block::default()
        .title(format!(" Stages ({}) ", stages.len()))
        .title_style(Theme::header())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BORDER));

    if stages.is_empty() {
        let empty = Paragraph::new("No stages").style(Theme::dimmed()).block(block);
        frame.render_widget(empty, area);
        return;
    }

    let header = Row::new(vec!["", "ID", "Status", "Merged", "Deps", "Elapsed"])
        .style(Theme::header())
        .bottom_margin(1);

    let rows: Vec<Row> = stages
        .iter()
        .map(|stage| {
            let icon = status_indicator(&stage.status);
            let status_str = status_text(&stage.status);
            let merged_str = if stage.merged { "✓" } else { "○" };

            let deps_str = if stage.dependencies.is_empty() {
                "-".to_string()
            } else {
                stage.dependencies.len().to_string()
            };

            let elapsed_str = stage
                .started_at
                .map(|t| {
                    let elapsed = chrono::Utc::now().signed_duration_since(t).num_seconds();
                    format_elapsed(elapsed)
                })
                .unwrap_or_else(|| "-".to_string());

            let style = match stage.status {
                StageStatus::Executing => Theme::status_executing(),
                StageStatus::Completed => Theme::status_completed(),
                StageStatus::Blocked
                | StageStatus::MergeConflict
                | StageStatus::MergeBlocked => Theme::status_blocked(),
                StageStatus::NeedsHandoff
                | StageStatus::WaitingForInput
                | StageStatus::CompletedWithFailures => Theme::status_warning(),
                StageStatus::Queued => Theme::status_queued(),
                _ => Theme::dimmed(),
            };

            Row::new(vec![
                icon.content.to_string(),
                stage.id.clone(),
                status_str.to_string(),
                merged_str.to_string(),
                deps_str,
                elapsed_str,
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),  // Icon
        Constraint::Min(20),    // ID
        Constraint::Length(10), // Status
        Constraint::Length(6),  // Merged
        Constraint::Length(4),  // Deps
        Constraint::Length(8),  // Elapsed
    ];

    let table = Table::new(rows, widths).block(block).header(header);
    frame.render_widget(table, area);
}

/// Render compact footer
fn render_compact_footer(frame: &mut Frame, area: Rect, last_error: &Option<String>) {
    let footer_line = if let Some(ref err) = last_error {
        Line::from(vec![
            Span::styled("Error: ", Style::default().fg(StatusColors::BLOCKED)),
            Span::styled(err.as_str(), Style::default().fg(StatusColors::BLOCKED)),
        ])
    } else {
        Line::from(vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" quit │ "),
            Span::styled("Daemon runs in background", Theme::dimmed()),
        ])
    };

    let footer = Paragraph::new(footer_line);
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
