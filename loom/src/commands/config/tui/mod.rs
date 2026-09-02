//! Interactive, staged editing for the typed user-config registry.
//!
//! This module deliberately keeps crossterm concerns at the edge: `state`
//! owns the headless state machine, and `render` only turns that state into
//! widgets. That split lets validation and disk writes have ordinary unit
//! tests without putting a test terminal into raw mode.

/// Draws immutable editor state with the shared status-screen palette.
mod render;
/// Owns navigation, validation, and writes without terminal dependencies.
mod state;

/// Headless coverage for navigation, validation, and the real config write seam.
#[cfg(test)]
mod tests;

use std::{
    io::{self, Stdout},
    time::Duration,
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use self::state::ConfigState;

/// Run the terminal editor for bare interactive `loom config` invocations.
pub fn run() -> Result<()> {
    let state = ConfigState::load()?;
    let mut app = ConfigTui::new(state)?;

    // Ctrl+C already returns from `handle_key` because raw mode delivers it
    // as an ordinary `KeyEvent`, never as SIGINT. This handler is for
    // termination that does not go through the terminal at all - an
    // external `kill`, a multiplexer pane closing, or a session manager
    // sending SIGTERM/SIGHUP - which skip unwinding entirely, so `Drop`
    // never runs and the operator's shell is left in raw alternate-screen
    // mode. `cleanup_terminal_crossterm` writes straight to stdout instead
    // of through `self.terminal` because the signal can land mid-render,
    // while `self.terminal`'s borrow is held elsewhere.
    ctrlc::set_handler(|| {
        crate::utils::cleanup_terminal_crossterm();
        std::process::exit(0);
    })
    .context("Failed to set signal handler")?;

    let result = app.event_loop();
    app.cleanup_terminal();
    result
}

/// Own the alternate-screen terminal while forwarding all edit decisions to state.
struct ConfigTui {
    /// The crossterm-backed terminal that draws this session.
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// The terminal-independent registry rows and staged edits.
    state: ConfigState,
    /// Prevent both explicit cleanup and `Drop` from restoring the terminal twice.
    cleaned_up: bool,
}

impl ConfigTui {
    /// Enter raw alternate-screen mode in the same order as the status TUI.
    ///
    /// A half-initialized terminal has no `Drop` to restore it: `Self` does
    /// not exist until this function returns `Ok`, so an error here must
    /// undo exactly what already succeeded before propagating, or the
    /// caller's terminal is left in raw mode (possibly on the alternate
    /// screen) with nothing left to run `cleanup_terminal`.
    fn new(state: ConfigState) -> Result<Self> {
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error).context("Failed to enter alternate screen");
        }

        crate::utils::install_crossterm_panic_hook();

        let backend = CrosstermBackend::new(stdout);
        let terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                return Err(error).context("Failed to create terminal");
            }
        };
        Ok(Self {
            terminal,
            state,
            cleaned_up: false,
        })
    }

    /// Draw frames and process input until a top-level quit command arrives.
    fn event_loop(&mut self) -> Result<()> {
        loop {
            self.draw()?;
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Release && self.handle_key(key) {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Translate terminal keys while preserving the state machine's headless boundary.
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return true;
        }
        if self.state.is_editing() {
            return self.handle_edit_key(key.code);
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => true,
            KeyCode::Up | KeyCode::Char('k') => {
                self.state.move_up();
                false
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.state.move_down();
                false
            }
            KeyCode::Enter => {
                self.state.begin_edit();
                false
            }
            KeyCode::Char('s') => {
                self.state.save();
                false
            }
            _ => false,
        }
    }

    /// Handle the small inline editor without allowing its keys to quit the TUI.
    fn handle_edit_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(character) => self.state.append_char(character),
            KeyCode::Backspace => self.state.backspace(),
            KeyCode::Enter => self.state.commit_edit(),
            KeyCode::Esc => self.state.cancel_edit(),
            _ => {}
        }
        false
    }

    /// Restore the caller's terminal exactly once, including after an early error.
    fn cleanup_terminal(&mut self) {
        if self.cleaned_up {
            return;
        }
        self.cleaned_up = true;

        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }

    /// Draw the current state into the terminal's next frame.
    fn draw(&mut self) -> Result<()> {
        self.terminal
            .draw(|frame| render::draw(frame, &self.state))?;
        Ok(())
    }
}

/// Ensure error propagation cannot strand the caller in raw alternate-screen mode.
impl Drop for ConfigTui {
    fn drop(&mut self) {
        self.cleanup_terminal();
    }
}
