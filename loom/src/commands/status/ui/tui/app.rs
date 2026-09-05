//! TUI application state and main loop.

use std::io::{self, Stdout};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::daemon_client::{connect, is_socket_disconnected, subscribe};
use super::event_handler::{handle_key_event, handle_mouse_event, is_scroll_key, KeyEventResult};
use super::ledger::{self, LedgerView};
use super::renderer::render_completion;
use super::state::{GraphState, LiveStatus, TuiActivityLog};
use crate::commands::status::render::attention_model::attention_entries;
use crate::commands::status::render::print_completion_summary;
use crate::daemon::{
    read_auth_token, read_message, write_message, CompletionSummary, Request, Response,
};
use crate::orchestrator::scheduling_report::{self, Alert};

/// Poll timeout for event loop (100ms for responsive UI).
const POLL_TIMEOUT: Duration = Duration::from_millis(100);

/// Delay before auto-exit after completion (500ms).
const COMPLETION_EXIT_DELAY: Duration = Duration::from_millis(500);

/// How often the alert files are re-read from the state directory.
const ALERT_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// Connect attempts before giving up - a busy `accept()` can fail once without being dead.
const RECONNECT_ATTEMPTS: u32 = 3;

/// Backoff between attempts - well under the 100ms input-poll interval.
const RECONNECT_BACKOFF: Duration = Duration::from_millis(80);

/// TUI application state.
pub struct TuiApp {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    running: Arc<AtomicBool>,
    status: LiveStatus,
    spinner_frame: usize,
    last_error: Option<String>,
    graph_state: GraphState,
    activity_log: TuiActivityLog,
    legend_open: bool,
    tick_age_secs: Option<i64>,
    mouse_enabled: bool,
    exiting: bool,
    completion_summary: Option<CompletionSummary>,
    /// Tracks when completion was received for auto-exit delay.
    completion_received_at: Option<Instant>,
    /// Flag to prevent double cleanup in Drop.
    cleaned_up: bool,
    alerts: Vec<Alert>,
    alerts_refreshed_at: Option<Instant>,
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
            activity_log: TuiActivityLog::new(),
            legend_open: false,
            tick_age_secs: None,
            mouse_enabled,
            exiting: false,
            completion_summary: None,
            completion_received_at: None,
            cleaned_up: false,
            alerts: Vec::new(),
            alerts_refreshed_at: None,
        })
    }

    /// Run the TUI event loop.
    pub fn run(&mut self, work_path: &Path) -> Result<()> {
        let mut stream = Self::connect_and_subscribe(work_path)?;
        crate::utils::write_tui_marker(work_path);
        self.install_exit_handler()?;
        let result = self.run_event_loop(&mut stream, work_path);
        self.unsubscribe(&mut stream, work_path);
        self.cleanup_terminal();
        if let Some(summary) = self.completion_summary.take() {
            print_completion_summary(&summary);
        }
        result
    }

    fn connect_and_subscribe(work_path: &Path) -> Result<UnixStream> {
        let socket_path = work_path.join("orchestrator.sock");
        let mut stream = connect(&socket_path)?;
        subscribe(&mut stream)?;
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .ok();
        Ok(stream)
    }

    fn install_exit_handler(&self) -> Result<()> {
        let running = self.running.clone();
        ctrlc::set_handler(move || {
            running.store(false, Ordering::SeqCst);
            crate::utils::cleanup_terminal_crossterm();
            std::process::exit(0);
        })
        .context("Failed to set Ctrl+C handler")
    }

    fn unsubscribe(&self, stream: &mut UnixStream, work_path: &Path) {
        let token = read_auth_token(work_path).unwrap_or_default();
        let _ = write_message(stream, &Request::Unsubscribe { auth_token: token });
    }

    /// Refresh scheduler alerts from the state directory, at most once per second.
    ///
    /// The event loop spins every ~50ms; the alert files change at the
    /// orchestrator's 5s poll rate, so re-reading them per frame would be
    /// twenty times the I/O for the same answer.
    fn refresh_alerts(&mut self, work_path: &Path) {
        let due = self
            .alerts_refreshed_at
            .is_none_or(|at| at.elapsed() >= ALERT_REFRESH_INTERVAL);
        if !due {
            return;
        }

        // The TUI only runs against a live daemon, so the stall check applies.
        self.alerts = scheduling_report::alerts(work_path, true);
        self.tick_age_secs = crate::orchestrator::tick::read(work_path)
            .ok()
            .flatten()
            .map(|tick| tick.age_secs(Utc::now()));
        self.alerts_refreshed_at = Some(Instant::now());
    }

    /// Main event loop - returns on quit or daemon disconnect.
    fn run_event_loop(&mut self, stream: &mut UnixStream, work_path: &Path) -> Result<()> {
        while self.running.load(Ordering::SeqCst) {
            self.refresh_alerts(work_path);

            if self.exiting {
                self.last_error = Some("Exiting...".to_string());
                self.render()?;
                break;
            }

            if self.completion_delay_elapsed() || self.receive_response(stream, work_path)? {
                break;
            }
            self.handle_input()?;

            self.spinner_frame = (self.spinner_frame + 1) % 10;

            self.render()?;
        }

        Ok(())
    }

    fn completion_delay_elapsed(&self) -> bool {
        self.completion_received_at
            .is_some_and(|received_at| received_at.elapsed() >= COMPLETION_EXIT_DELAY)
    }

    /// Read one daemon response and apply it. A read timeout with nothing
    /// consumed is the normal idle case (~20x/second) and stays silent -
    /// `is_read_timeout` is load-bearing here. It also swallows a timeout
    /// landing mid-frame (`read_exact` cannot tell the two apart), losing one
    /// frame; recovery happens on the *next* read, once the misaligned stream
    /// makes `read_frame_length` (`daemon/wire.rs:190-199`) decode a length
    /// `>= 0x20000000` from JSON body bytes - past `MAX_RESPONSE_BYTES` (2
    /// MiB) - reaching `reconnect_after_read_error` below (pinned by
    /// `app_tests.rs:24-32`).
    fn receive_response(&mut self, stream: &mut UnixStream, work_path: &Path) -> Result<bool> {
        match read_message::<Response, _>(stream) {
            Ok(response) => self.handle_response(response),
            Err(error) if is_socket_disconnected(&error) => {
                if self.completion_summary.is_some() {
                    return Ok(true);
                }
                self.last_error = Some("Daemon exited".to_string());
                self.render()?;
                std::thread::sleep(Duration::from_millis(500));
                return Ok(true);
            }
            Err(error) if is_read_timeout(&error) => {}
            Err(error) => return self.reconnect_after_read_error(&error, stream, work_path),
        }
        Ok(false)
    }

    /// Tear down the stream and resubscribe; reports the daemon dead only
    /// after `RECONNECT_ATTEMPTS` straight attempts fail.
    fn reconnect_after_read_error(
        &mut self,
        error: &anyhow::Error,
        stream: &mut UnixStream,
        work_path: &Path,
    ) -> Result<bool> {
        self.last_error = Some(format!("Reconnecting after a status read error: {error}"));
        for attempt in 1..=RECONNECT_ATTEMPTS {
            if let Ok(new_stream) = Self::connect_and_subscribe(work_path) {
                *stream = new_stream;
                return Ok(false);
            }
            if attempt < RECONNECT_ATTEMPTS {
                std::thread::sleep(RECONNECT_BACKOFF);
            }
        }
        self.last_error = Some("Daemon exited".to_string());
        self.render()?;
        std::thread::sleep(Duration::from_millis(500));
        Ok(true)
    }

    fn handle_input(&mut self) -> Result<()> {
        if !event::poll(POLL_TIMEOUT)? {
            return Ok(());
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.handle_key(key.code, key.modifiers);
            }
            Event::Mouse(mouse) if !self.legend_open => {
                handle_mouse_event(mouse.kind, &mut self.graph_state)
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let result = if self.legend_open && is_scroll_key(code) {
            KeyEventResult::Continue
        } else {
            handle_key_event(code, modifiers, &mut self.graph_state)
        };
        match result {
            KeyEventResult::Exit => self.exiting = true,
            KeyEventResult::ToggleLegend => self.legend_open = !self.legend_open,
            KeyEventResult::CloseLegend if self.legend_open => self.legend_open = false,
            KeyEventResult::CloseLegend => self.exiting = true,
            KeyEventResult::Continue => {}
        }
    }

    /// Handle a response from the daemon.
    fn handle_response(&mut self, response: Response) {
        match response {
            Response::StatusUpdate { data } => {
                self.status = LiveStatus { data: *data };
                let all_stages = self.status.all_stages();
                self.activity_log.update(&all_stages);
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

        let levels = self.status.compute_levels();
        let ordered = self.status.all_stages_with_levels(&levels);
        let attention = attention_entries(&self.status.data.stages);
        let view = LedgerView {
            data: &self.status.data,
            levels: &levels,
            ordered: &ordered,
            attention: &attention,
            activity: &self.activity_log,
            alerts: &self.alerts,
            spinner: self.spinner_char(),
            scroll_y: self.graph_state.scroll_y,
            legend_open: self.legend_open,
            tick_age_secs: self.tick_age_secs,
            last_error: self.last_error.as_deref(),
            now_epoch: Utc::now().timestamp(),
            // The previous frame's viewport; one frame of lag is fine.
            scrollable: ordered.len() > usize::from(self.graph_state.viewport_height),
        };
        let mut outcome = ledger::RenderOutcome::default();
        self.terminal
            .draw(|frame| outcome = ledger::render(frame, &view))?;
        self.graph_state.total_lines = ordered.len().min(u16::MAX as usize) as u16;
        self.graph_state.viewport_height = outcome.table_viewport_rows;

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

/// True when `error`'s underlying `io::Error` is a read timeout
/// (`WouldBlock`/`TimedOut`) - the kind a 50ms socket read timeout produces
/// both for the ordinary idle case and for a timeout that landed mid-frame.
/// `daemon_client::is_socket_disconnected` answers a different question (is
/// this a real disconnect) and returns false for both timeout kinds too, so
/// it cannot serve here.
fn is_read_timeout(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io_error| {
                matches!(
                    io_error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                )
            })
    })
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        self.cleanup_terminal();
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
