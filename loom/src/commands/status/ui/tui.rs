//! TUI application for live status dashboard
//!
//! This module provides the ratatui-based terminal UI for displaying
//! live status updates from the loom daemon.

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
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table},
    Frame, Terminal,
};

use crate::daemon::{read_message, write_message, Request, Response, StageInfo};

use super::theme::{StatusColors, Theme};

/// Connection timeout for daemon socket
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

/// Poll timeout for event loop (100ms for responsive UI)
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

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

        self.terminal.draw(|frame| {
            let area = frame.area();

            // Main layout: header, progress, content, footer
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header
                    Constraint::Length(3), // Progress bar
                    Constraint::Min(10),   // Main content
                    Constraint::Length(3), // Footer
                ])
                .split(area);

            render_header(frame, chunks[0], spinner);
            render_progress(frame, chunks[1], pct, completed_count, total);
            render_stages(frame, chunks[2], &executing, &pending, &completed, &blocked);
            render_footer(frame, chunks[3], &last_error);
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

/// Render the header
fn render_header(frame: &mut Frame, area: Rect, spinner: char) {
    let title = format!("{spinner} Loom Live Status Dashboard");

    let header = Paragraph::new(title)
        .style(Theme::header())
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(StatusColors::BORDER)),
        );

    frame.render_widget(header, area);
}

/// Render the progress bar
fn render_progress(frame: &mut Frame, area: Rect, pct: f64, completed_count: usize, total: usize) {
    let label = format!(
        "{}/{} stages completed ({:.0}%)",
        completed_count,
        total,
        pct * 100.0
    );

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(StatusColors::COMPLETED))
        .ratio(pct)
        .label(label);

    frame.render_widget(gauge, area);
}

/// Render the stages panel
fn render_stages(
    frame: &mut Frame,
    area: Rect,
    executing: &[StageInfo],
    pending: &[String],
    completed: &[String],
    blocked: &[String],
) {
    // Split into columns
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Executing + Pending
            Constraint::Percentage(50), // Completed + Blocked
        ])
        .split(area);

    // Left column: Executing and Pending
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[0]);

    render_executing(frame, left_chunks[0], executing);
    render_pending(frame, left_chunks[1], pending);

    // Right column: Completed and Blocked
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);

    render_completed(frame, right_chunks[0], completed);
    render_blocked(frame, right_chunks[1], blocked);
}

/// Render executing stages
fn render_executing(frame: &mut Frame, area: Rect, executing: &[StageInfo]) {
    let title = format!(" Executing ({}) ", executing.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(StatusColors::EXECUTING).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::EXECUTING));

    if executing.is_empty() {
        let empty = Paragraph::new("No stages executing")
            .style(Theme::dimmed())
            .block(block);
        frame.render_widget(empty, area);
    } else {
        let rows: Vec<Row> = executing
            .iter()
            .map(|stage| {
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(stage.started_at)
                    .num_seconds();
                let time_str = format_elapsed(elapsed);
                let pid_str = stage
                    .session_pid
                    .map(|p| format!("PID:{p}"))
                    .unwrap_or_default();

                Row::new(vec![
                    format!("● {}", stage.id),
                    stage.name.clone(),
                    time_str,
                    pid_str,
                ])
                .style(Style::default().fg(StatusColors::EXECUTING))
            })
            .collect();

        let widths = [
            Constraint::Length(20),
            Constraint::Min(15),
            Constraint::Length(10),
            Constraint::Length(12),
        ];

        let table = Table::new(rows, widths)
            .block(block)
            .header(
                Row::new(vec!["ID", "Name", "Elapsed", "Session"])
                    .style(Theme::header())
                    .bottom_margin(1),
            );

        frame.render_widget(table, area);
    }
}

/// Render pending stages
fn render_pending(frame: &mut Frame, area: Rect, pending: &[String]) {
    let title = format!(" Pending ({}) ", pending.len());
    let block = Block::default()
        .title(title)
        .title_style(Theme::dimmed())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::PENDING));

    let items: Vec<ListItem> = pending
        .iter()
        .take(10)
        .map(|id| ListItem::new(format!("○ {id}")).style(Theme::dimmed()))
        .collect();

    let mut items = items;
    if pending.len() > 10 {
        items.push(
            ListItem::new(format!("  ... {} more", pending.len() - 10))
                .style(Theme::dimmed()),
        );
    }

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

/// Render completed stages
fn render_completed(frame: &mut Frame, area: Rect, completed: &[String]) {
    let title = format!(" Completed ({}) ", completed.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(StatusColors::COMPLETED))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::COMPLETED));

    let items: Vec<ListItem> = completed
        .iter()
        .take(10)
        .map(|id| ListItem::new(format!("✓ {id}")).style(Style::default().fg(StatusColors::COMPLETED)))
        .collect();

    let mut items = items;
    if completed.len() > 10 {
        items.push(
            ListItem::new(format!("  ... {} more", completed.len() - 10))
                .style(Theme::dimmed()),
        );
    }

    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

/// Render blocked stages
fn render_blocked(frame: &mut Frame, area: Rect, blocked: &[String]) {
    let title = format!(" Blocked ({}) ", blocked.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(StatusColors::BLOCKED).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(StatusColors::BLOCKED));

    if blocked.is_empty() {
        let empty = Paragraph::new("No blocked stages")
            .style(Theme::dimmed())
            .block(block);
        frame.render_widget(empty, area);
    } else {
        let items: Vec<ListItem> = blocked
            .iter()
            .map(|id| {
                ListItem::new(format!("✗ {id}"))
                    .style(Style::default().fg(StatusColors::BLOCKED))
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }
}

/// Render the footer
fn render_footer(frame: &mut Frame, area: Rect, last_error: &Option<String>) {
    let help_text = if let Some(ref err) = last_error {
        Line::from(vec![
            Span::styled("Error: ", Style::default().fg(StatusColors::BLOCKED)),
            Span::styled(err.as_str(), Style::default().fg(StatusColors::BLOCKED)),
        ])
    } else {
        Line::from(vec![
            Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("/"),
            Span::styled("Esc", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" quit  "),
            Span::styled("Daemon continues in background", Theme::dimmed()),
        ])
    };

    let footer = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(StatusColors::BORDER)),
    );

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
