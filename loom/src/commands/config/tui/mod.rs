//! Interactive, staged editing for the typed user-config registry.
//!
//! This module deliberately keeps crossterm concerns at the edge: `state`
//! owns the headless state machine, and `render` only turns that state into
//! widgets. That split lets validation and disk writes have ordinary unit
//! tests without putting a test terminal into raw mode.

/// Draws immutable editor state with this screen's own palette.
mod render;
/// Owns navigation, validation, and writes without terminal dependencies.
mod state;
/// The palette and style constructors both tiers are drawn with.
mod theme;

/// Headless coverage for navigation, validation, and the real config write seam.
#[cfg(test)]
mod tests;

use std::{
    io::{self, Stdout},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
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
    // sending SIGTERM/SIGHUP. It only clears `running`: `event_loop` sees
    // that within one 100 ms poll, `cleanup_terminal` restores the terminal
    // from the main thread, and the process then exits 130.
    let running = app.running.clone();
    ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))
        .context("Failed to set signal handler")?;

    let result = app.event_loop();
    app.cleanup_terminal();
    if !app.running.load(Ordering::SeqCst) {
        std::process::exit(130);
    }
    result
}

/// Own the alternate-screen terminal while forwarding all edit decisions to state.
struct ConfigTui {
    /// The crossterm-backed terminal that draws this session.
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// The terminal-independent registry rows and staged edits.
    state: ConfigState,
    /// Cleared by the signal handler; `event_loop` leaves when it is false.
    running: Arc<AtomicBool>,
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
            running: Arc::new(AtomicBool::new(true)),
            cleaned_up: false,
        })
    }

    /// Draw frames and process input until a top-level quit command arrives.
    fn event_loop(&mut self) -> Result<()> {
        while self.running.load(Ordering::SeqCst) {
            self.draw()?;
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Release && self.handle_key(key) {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Translate terminal keys while preserving the state machine's headless boundary.
    fn handle_key(&mut self, key: KeyEvent) -> bool {
        dispatch_key(&mut self.state, key)
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

/// Translate a key event into a state-machine operation without a live terminal.
fn dispatch_key(state: &mut ConfigState, key: KeyEvent) -> bool {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return true;
    }
    if state.is_editing() {
        return dispatch_edit_key(state, key);
    }

    // Whether a quit may proceed is the state machine's decision, not this
    // function's: it depends on what is staged on BOTH tabs, and it has to be
    // testable without a terminal. `Esc` goes through its own entry point
    // because dismissing an armed warning and confirming a discard are
    // opposite outcomes that must not share one bool.
    if key.code == KeyCode::Char('q') {
        return state.request_quit();
    }
    if key.code == KeyCode::Esc {
        return state.dismiss_or_request_quit();
    }
    dispatch_browse_key(state, key.code);
    false
}

/// Move the selection, switch tier, or act on the focused row. Every key here
/// keeps the editor open; quitting is decided by the caller, so no arm has to
/// carry a `false` to say so.
fn dispatch_browse_key(state: &mut ConfigState, code: KeyCode) {
    // Any key that is not a second quit means the operator is still working,
    // so a refused quit stops counting toward the next one.
    state.disarm_quit();
    match code {
        KeyCode::Up | KeyCode::Char('k') => state.move_up(),
        KeyCode::Down | KeyCode::Char('j') => state.move_down(),
        KeyCode::Char('g') => state.move_to_first(),
        KeyCode::Char('G') => state.move_to_last(),
        KeyCode::Left | KeyCode::Char('h') => state.cycle(-1),
        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => state.cycle(1),
        KeyCode::Tab | KeyCode::BackTab => state.toggle_scope(),
        KeyCode::Char('x') | KeyCode::Delete => state.stage_clear(),
        KeyCode::Enter => state.activate(),
        KeyCode::Char('s') => state.save(),
        _ => {}
    }
}

/// Handle a key while a text editor owns the keyboard. Control characters and
/// `Ctrl`/`Alt` chords are not text and never reach the buffer.
fn dispatch_edit_key(state: &mut ConfigState, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char(character) => {
            let chord = key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
            if !character.is_control() && !chord {
                state.append_char(character);
            }
        }
        KeyCode::Backspace => state.backspace(),
        KeyCode::Enter => state.commit_edit(),
        KeyCode::Esc => state.cancel_edit(),
        _ => {}
    }
    false
}
