//! TUI application state and main loop.

use std::collections::HashMap;
use std::io::{self, Stdout};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    Terminal,
};

use super::daemon_client::{connect, is_socket_disconnected, subscribe};
use super::event_handler::{handle_key_event, handle_mouse_event, KeyEventResult};
use super::renderer::{
    render_compact_footer, render_compact_header, render_completion, render_tree_graph,
    render_unified_table, unified_stage_to_stage, GRAPH_AREA_HEIGHT,
};
use super::state::{GraphState, LiveStatus};
use crate::commands::status::render::print_completion_summary;
use crate::daemon::{
    read_auth_token, read_message, write_message, CompletionSummary, Request, Response,
};
use crate::models::stage::StageStatus;

/// Poll timeout for event loop (100ms for responsive UI).
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Delay before auto-exit after completion (500ms).
const COMPLETION_EXIT_DELAY: Duration = Duration::from_millis(500);

/// TUI application state.
pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: Arc<AtomicBool>,
    status: LiveStatus,
    spinner_frame: usize,
    last_error: Option<String>,
    graph_state: GraphState,
    mouse_enabled: bool,
    exiting: bool,
    completion_summary: Option<CompletionSummary>,
    /// Tracks when completion was received for auto-exit delay.
    completion_received_at: Option<Instant>,
    /// Flag to prevent double cleanup in Drop.
    cleaned_up: bool,
}

impl TuiApp {
    /// Create a new TUI application.
    pub fn new() -> Result<Self> {
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;

        crate::utils::install_crossterm_panic_hook();

        let mouse_enabled =
            crossterm::execute!(stdout, crossterm::event::EnableMouseCapture).is_ok();

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
            exiting: false,
            completion_summary: None,
            completion_received_at: None,
            cleaned_up: false,
        })
    }

    /// Run the TUI event loop.
    pub fn run(&mut self, work_path: &Path) -> Result<()> {
        let socket_path = work_path.join("orchestrator.sock");
        let mut stream = connect(&socket_path)?;
        subscribe(&mut stream)?;

        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .ok();

        // Install Ctrl+C handler to ensure terminal cleanup on signal
        let running = self.running.clone();

        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);

            // Cleanup crossterm state - must be done in signal handler
            // since Drop may not run on process exit
            crate::utils::cleanup_terminal_crossterm();

            std::process::exit(0);
        })
        .context("Failed to set Ctrl+C handler")?;

        let result = self.run_event_loop(&mut stream);

        let token = read_auth_token(std::path::Path::new(".work")).unwrap_or_default();
        let _ = write_message(&mut stream, &Request::Unsubscribe { auth_token: token });

        // ALWAYS cleanup terminal before returning - prevents terminal state
        // corruption when daemon dies without sending OrchestrationComplete
        self.cleanup_terminal();

        // Print completion summary AFTER cleanup since it needs normal terminal
        if let Some(summary) = self.completion_summary.take() {
            print_completion_summary(&summary);
        }

        result
    }

    /// Main event loop - returns on quit or daemon disconnect.
    fn run_event_loop(&mut self, stream: &mut UnixStream) -> Result<()> {
        while self.running.load(Ordering::SeqCst) {
            if self.exiting {
                self.last_error = Some("Exiting...".to_string());
                self.render()?;
                break;
            }

            // Check if completion delay has elapsed for auto-exit
            if let Some(received_at) = self.completion_received_at {
                if received_at.elapsed() >= COMPLETION_EXIT_DELAY {
                    // Delay elapsed, exit cleanly
                    break;
                }
            }

            match read_message::<Response, _>(stream) {
                Ok(response) => {
                    self.handle_response(response);
                }
                Err(e) => {
                    if is_socket_disconnected(&e) {
                        // If we already have completion summary, don't show error
                        if self.completion_summary.is_some() {
                            break;
                        }
                        self.last_error = Some("Daemon exited".to_string());
                        self.render()?;
                        std::thread::sleep(Duration::from_millis(500));
                        break;
                    }
                }
            }

            if event::poll(POLL_TIMEOUT)? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        match handle_key_event(key.code, key.modifiers, &mut self.graph_state) {
                            KeyEventResult::Exit => self.exiting = true,
                            KeyEventResult::Continue => {}
                        }
                    }
                    Event::Mouse(mouse) => {
                        handle_mouse_event(mouse.kind, &mut self.graph_state);
                    }
                    _ => {}
                }
            }

            self.spinner_frame = (self.spinner_frame + 1) % 10;

            self.render()?;
        }

        Ok(())
    }

    /// Handle a response from the daemon.
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
            Response::OrchestrationComplete { summary } => {
                self.completion_summary = Some(summary);
                self.completion_received_at = Some(Instant::now());
                self.last_error = None;
            }
            Response::Error { message } => {
                self.last_error = Some(message);
            }
            _ => {}
        }
    }

    /// Cleanup terminal state (leave alternate screen, disable raw mode).
    /// Sets cleaned_up flag to prevent double cleanup in Drop.
    fn cleanup_terminal(&mut self) {
        if self.cleaned_up {
            return;
        }
        self.cleaned_up = true;

        let _ = disable_raw_mode();
        if self.mouse_enabled {
            let _ = crossterm::execute!(
                self.terminal.backend_mut(),
                crossterm::event::DisableMouseCapture
            );
        }
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }

    /// Render the UI.
    fn render(&mut self) -> Result<()> {
        // If orchestration completed, render completion screen
        if let Some(ref summary) = self.completion_summary {
            let summary_clone = summary.clone();
            self.terminal.draw(|frame| {
                render_completion(frame, frame.area(), &summary_clone);
            })?;
            return Ok(());
        }

        let spinner = self.spinner_char();
        let status = &self.status;
        let last_error = self.last_error.clone();

        let pct = status.progress_pct();
        let total = status.total();
        let completed_count = status.completed.len();

        let unified_stages = status.unified_stages();

        let stages_for_graph: Vec<_> = unified_stages.iter().map(unified_stage_to_stage).collect();

        let total_lines = unified_stages.iter().fold(0_u16, |acc, s| {
            let base = 1;
            let extra = if matches!(s.status, StageStatus::Executing | StageStatus::Queued) {
                1
            } else {
                0
            };
            acc + base + extra
        });
        self.graph_state.total_lines = total_lines;

        let scroll_y = self.graph_state.scroll_y;

        let context_pcts = HashMap::new();
        let mut elapsed_times = HashMap::new();
        for stage in &unified_stages {
            if let (Some(start), StageStatus::Executing) = (stage.started_at, &stage.status) {
                let elapsed = chrono::Utc::now()
                    .signed_duration_since(start)
                    .num_seconds();
                elapsed_times.insert(stage.id.clone(), elapsed);
            }
        }

        self.terminal.draw(|frame| {
            let area = frame.area();

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(5),
                    Constraint::Length(1),
                    Constraint::Length(GRAPH_AREA_HEIGHT),
                    Constraint::Length(1),
                    Constraint::Min(6),
                    Constraint::Length(1),
                ])
                .split(area);

            render_compact_header(frame, chunks[0], spinner, pct, completed_count, total);

            render_tree_graph(
                frame,
                chunks[2],
                &stages_for_graph,
                scroll_y,
                &context_pcts,
                &elapsed_times,
            );

            render_unified_table(frame, chunks[4], &unified_stages);
            render_compact_footer(frame, chunks[5], &last_error);
        })?;

        self.graph_state.viewport_height = GRAPH_AREA_HEIGHT.saturating_sub(2);

        Ok(())
    }

    /// Get spinner character for current frame.
    fn spinner_char(&self) -> char {
        const SPINNER: [char; 10] = [
            '\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}',
            '\u{2827}', '\u{2807}', '\u{280F}',
        ];
        SPINNER[self.spinner_frame % SPINNER.len()]
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        self.cleanup_terminal();
    }
}
