//! TUI application state and main loop.

use std::io::{self, Stdout};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::daemon_client::{connect, is_socket_disconnected, subscribe, user_token};
use super::event_handler::{handle_key_event, handle_mouse_event, is_scroll_key, KeyEventResult};
use super::ledger::{self, LedgerView};
use super::plan_overview::{render_plan_overview, scroll_alias};
use super::renderer::render_completion;
use super::state::{GraphState, LiveStatus, TuiActivityLog};
use crate::commands::status::render::attention_model::attention_entries;
use crate::commands::status::render::print_completion_summary;
use crate::daemon::{write_message, CompletionSummary, Request, Response, ResponseFrameReader};
use crate::orchestrator::busy;
use crate::orchestrator::scheduling_report::{self, Alert};
use crate::orchestrator::tick::{self, Tick};
use crate::plan_stats::{read_current, PlanStatsRecord};

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

#[path = "app_terminal.rs"]
mod app_terminal;
use app_terminal::TerminalGuard;

#[path = "app_input.rs"]
mod app_input;
#[path = "app_view.rs"]
mod app_view;

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
    /// `None` only for the headless test app, which is never drawn to a terminal.
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    running: Arc<AtomicBool>,
    status: LiveStatus,
    spinner_frame: usize,
    last_error: Option<String>,
    graph_state: GraphState,
    activity_log: TuiActivityLog,
    legend_open: bool,
    show_tokens: bool,
    tick: Option<Tick>,
    terminal_guard: TerminalGuard,
    exiting: bool,
    completion_summary: Option<CompletionSummary>,
    /// Tracks when completion was received for auto-exit delay.
    completion_received_at: Option<Instant>,
    /// Flag to prevent double cleanup in Drop.
    cleaned_up: bool,
    alerts: Vec<Alert>,
    alerts_refreshed_at: Option<Instant>,
    /// The finished plan's report; while set it replaces the ledger and the
    /// legacy completion view.
    plan_report: Option<PlanStatsRecord>,
    /// The daemon closed the socket; nothing more will arrive on it.
    socket_closed: bool,
}

impl TuiApp {
    /// Create a new TUI application.
    pub fn new() -> Result<Self> {
        let (terminal_guard, terminal) = app_terminal::initialize(|| {
            let backend = CrosstermBackend::new(io::stdout());
            Terminal::new(backend).context("Failed to create terminal")
        })?;

        Ok(Self {
            terminal: Some(terminal),
            running: Arc::new(AtomicBool::new(true)),
            status: LiveStatus::default(),
            spinner_frame: 0,
            last_error: None,
            graph_state: GraphState::default(),
            activity_log: TuiActivityLog::new(),
            legend_open: false,
            show_tokens: false,
            tick: None,
            terminal_guard,
            exiting: false,
            completion_summary: None,
            completion_received_at: None,
            cleaned_up: false,
            alerts: Vec::new(),
            alerts_refreshed_at: None,
            plan_report: None,
            socket_closed: false,
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
            if let Some(ref error) = self.last_error {
                eprintln!("{error}");
            }
            print_completion_summary(&summary);
        }
        if self.interrupted() {
            std::process::exit(130);
        }
        result
    }

    fn connect_and_subscribe(work_path: &Path) -> Result<UnixStream> {
        let mut stream = connect(work_path)?;
        subscribe(&mut stream, work_path)?;
        stream
            .set_read_timeout(Some(Duration::from_millis(50)))
            .ok();
        Ok(stream)
    }

    /// The handler only clears `running`; the event loop leaves on its next
    /// iteration and the caller restores the terminal. It also serves SIGTERM
    /// and SIGHUP (the `ctrlc` `termination` feature).
    fn install_exit_handler(&self) -> Result<()> {
        let running = self.running.clone();
        ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))
            .context("Failed to set Ctrl+C handler")
    }

    /// `running` is cleared only by the exit handler, so false means a signal arrived.
    fn interrupted(&self) -> bool {
        !self.running.load(Ordering::SeqCst)
    }

    fn unsubscribe(&self, stream: &mut UnixStream, work_path: &Path) {
        let auth_token = user_token(work_path);
        let _ = write_message(stream, &Request::Unsubscribe { auth_token });
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
        self.tick = tick::read(work_path).ok().flatten();
        self.alerts_refreshed_at = Some(Instant::now());
    }

    /// Main event loop - returns on quit or daemon disconnect.
    fn run_event_loop(&mut self, stream: &mut UnixStream, work_path: &Path) -> Result<()> {
        let mut reader = ResponseFrameReader::new(Duration::from_secs(30));
        while self.running.load(Ordering::SeqCst) {
            if !self.advance(stream, work_path, &mut reader)? {
                if self.exiting {
                    self.last_error = Some("Exiting...".to_string());
                    self.render()?;
                }
                break;
            }
            self.handle_input()?;

            self.spinner_frame = (self.spinner_frame + 1) % 10;

            self.render()?;
        }

        Ok(())
    }

    /// One event-loop iteration without input polling or drawing; `false`
    /// ends the session: the user quit, the legacy completion view ran its
    /// delay, or the daemon is gone and there is no report to keep showing.
    pub(super) fn advance(
        &mut self,
        stream: &mut UnixStream,
        work_path: &Path,
        reader: &mut ResponseFrameReader,
    ) -> Result<bool> {
        self.refresh_alerts(work_path);
        if self.exiting || self.completion_delay_elapsed() {
            return Ok(false);
        }
        if self.socket_closed {
            return Ok(true);
        }
        Ok(!self.receive_response(stream, work_path, reader)?)
    }

    /// The legacy completion view leaves on its own; the report stays until `q`.
    fn completion_delay_elapsed(&self) -> bool {
        self.plan_report.is_none()
            && self
                .completion_received_at
                .is_some_and(|received_at| received_at.elapsed() >= COMPLETION_EXIT_DELAY)
    }

    /// Show the run's saved report, if there is one.
    fn load_report(&mut self, work_path: &Path) {
        if let Some(record) = read_current(work_path) {
            self.show_report(record);
        }
    }

    /// Whether the plan ended in failure, which keeps the legacy completion view.
    fn completed_with_failures(&self) -> bool {
        self.completion_summary
            .as_ref()
            .is_some_and(|summary| summary.failure_count > 0)
    }

    /// Read one complete daemon response while preserving partial frames over idle polls.
    fn receive_response(
        &mut self,
        stream: &mut UnixStream,
        work_path: &Path,
        reader: &mut ResponseFrameReader,
    ) -> Result<bool> {
        match reader.poll(stream) {
            Ok(Some(response)) => {
                // The daemon may send only the completion, never a final status.
                let succeeded = matches!(
                    &response,
                    Response::OrchestrationComplete { summary } if summary.failure_count == 0
                );
                self.handle_response(response);
                if succeeded {
                    self.load_report(work_path);
                }
            }
            Ok(None) => {}
            Err(error) if is_socket_disconnected(&error) => {
                if self.plan_report.is_none() && !self.completed_with_failures() {
                    self.load_report(work_path);
                }
                if self.plan_report.is_some() {
                    self.socket_closed = true;
                    return Ok(false);
                }
                if self.completion_summary.is_some() {
                    self.socket_closed = true;
                    return Ok(true);
                }
                // The daemon may only have shut a slow subscriber: resubscribe,
                // and report it exited only when every attempt fails.
                let recovered = self.reconnect_after_read_error(&error, stream, work_path)?;
                *reader = ResponseFrameReader::new(Duration::from_secs(30));
                return Ok(recovered);
            }
            Err(error) => {
                let recovered = self.reconnect_after_read_error(&error, stream, work_path)?;
                *reader = ResponseFrameReader::new(Duration::from_secs(30));
                return Ok(recovered);
            }
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
            if self.interrupted() {
                return Ok(true);
            }
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

    /// Cleanup terminal state (leave alternate screen, disable raw mode).
    /// Sets cleaned_up flag to prevent double cleanup in Drop.
    fn cleanup_terminal(&mut self) {
        if self.cleaned_up {
            return;
        }
        self.cleaned_up = true;

        self.terminal_guard.cleanup();
        if let Some(terminal) = self.terminal.as_mut() {
            let _ = terminal.show_cursor();
        }
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        self.cleanup_terminal();
    }
}
