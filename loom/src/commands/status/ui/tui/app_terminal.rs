use std::io::{self, stdout};

use anyhow::{Context, Result};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};

pub(super) trait TerminalModes {
    fn raw(&mut self) -> io::Result<()>;
    fn alternate(&mut self) -> io::Result<()>;
    fn mouse(&mut self) -> io::Result<()>;
    fn reset_raw(&mut self);
    fn reset_alternate(&mut self);
    fn reset_mouse(&mut self);
}

pub(super) struct CrosstermModes;

impl TerminalModes for CrosstermModes {
    fn raw(&mut self) -> io::Result<()> {
        enable_raw_mode()
    }
    fn alternate(&mut self) -> io::Result<()> {
        execute!(stdout(), EnterAlternateScreen)
    }
    fn mouse(&mut self) -> io::Result<()> {
        execute!(stdout(), EnableMouseCapture)
    }
    fn reset_raw(&mut self) {
        let _ = disable_raw_mode();
    }
    fn reset_alternate(&mut self) {
        let _ = execute!(stdout(), LeaveAlternateScreen);
    }
    fn reset_mouse(&mut self) {
        let _ = execute!(stdout(), DisableMouseCapture);
    }
}

pub(super) struct TerminalGuard<M: TerminalModes = CrosstermModes> {
    modes: M,
    raw: bool,
    alternate: bool,
    mouse: bool,
}

impl<M: TerminalModes> TerminalGuard<M> {
    fn acquire(mut modes: M) -> Result<Self> {
        modes.raw().context("Failed to enable raw mode")?;
        let mut guard = Self {
            modes,
            raw: true,
            alternate: false,
            mouse: false,
        };
        guard
            .modes
            .alternate()
            .context("Failed to enter alternate screen")?;
        guard.alternate = true;
        guard.mouse = guard.modes.mouse().is_ok();
        Ok(guard)
    }

    pub(super) fn cleanup(&mut self) {
        if std::mem::take(&mut self.raw) {
            self.modes.reset_raw();
        }
        if std::mem::take(&mut self.mouse) {
            self.modes.reset_mouse();
        }
        if std::mem::take(&mut self.alternate) {
            self.modes.reset_alternate();
        }
    }
}

fn with_modes<M: TerminalModes, T>(
    modes: M,
    create: impl FnOnce() -> Result<T>,
) -> Result<(TerminalGuard<M>, T)> {
    let guard = TerminalGuard::acquire(modes)?;
    let terminal = create()?;
    Ok((guard, terminal))
}

pub(super) fn initialize<T>(create: impl FnOnce() -> Result<T>) -> Result<(TerminalGuard, T)> {
    with_modes(CrosstermModes, || {
        crate::utils::install_crossterm_panic_hook();
        create()
    })
}

impl<M: TerminalModes> Drop for TerminalGuard<M> {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    struct Mock {
        events: Rc<RefCell<Vec<&'static str>>>,
        fail_alternate: bool,
        fail_mouse: bool,
    }

    impl TerminalModes for Mock {
        fn raw(&mut self) -> io::Result<()> {
            self.events.borrow_mut().push("raw");
            Ok(())
        }
        fn alternate(&mut self) -> io::Result<()> {
            self.events.borrow_mut().push("alternate");
            if self.fail_alternate {
                Err(io::ErrorKind::Other.into())
            } else {
                Ok(())
            }
        }
        fn mouse(&mut self) -> io::Result<()> {
            self.events.borrow_mut().push("mouse");
            if self.fail_mouse {
                Err(io::ErrorKind::Other.into())
            } else {
                Ok(())
            }
        }
        fn reset_raw(&mut self) {
            self.events.borrow_mut().push("reset raw");
        }
        fn reset_alternate(&mut self) {
            self.events.borrow_mut().push("reset alternate");
        }
        fn reset_mouse(&mut self) {
            self.events.borrow_mut().push("reset mouse");
        }
    }

    fn mock(
        events: &Rc<RefCell<Vec<&'static str>>>,
        fail_alternate: bool,
        fail_mouse: bool,
    ) -> Mock {
        Mock {
            events: Rc::clone(events),
            fail_alternate,
            fail_mouse,
        }
    }

    #[test]
    fn alternate_failure_rolls_back_raw() {
        let events = Rc::default();
        assert!(TerminalGuard::acquire(mock(&events, true, false)).is_err());
        assert_eq!(*events.borrow(), ["raw", "alternate", "reset raw"]);
    }

    #[test]
    fn construction_failure_rolls_back_acquired_modes() {
        let events = Rc::default();
        let result = with_modes(mock(&events, false, false), || -> Result<()> {
            anyhow::bail!("terminal construction failed")
        });
        assert!(result.is_err());
        assert_eq!(
            *events.borrow(),
            [
                "raw",
                "alternate",
                "mouse",
                "reset raw",
                "reset mouse",
                "reset alternate"
            ]
        );
    }

    #[test]
    fn cleanup_and_drop_release_each_acquired_mode_once() {
        let events = Rc::default();
        {
            let mut guard = TerminalGuard::acquire(mock(&events, false, true)).unwrap();
            guard.cleanup();
            guard.cleanup();
        }
        assert_eq!(
            *events.borrow(),
            ["raw", "alternate", "mouse", "reset raw", "reset alternate"]
        );
    }

    #[test]
    fn successful_cleanup_then_drop_releases_all_modes_once() {
        let events = Rc::default();
        {
            let mut guard = TerminalGuard::acquire(mock(&events, false, false)).unwrap();
            guard.cleanup();
        }
        assert_eq!(
            *events.borrow(),
            [
                "raw",
                "alternate",
                "mouse",
                "reset raw",
                "reset mouse",
                "reset alternate"
            ]
        );
    }
}
