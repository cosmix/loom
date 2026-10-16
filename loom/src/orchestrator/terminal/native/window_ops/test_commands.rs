//! Thread-local command replies keep unit tests away from the host desktop.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::process::{Command, Output};

#[derive(Debug)]
pub(super) struct Invocation {
    pub program: OsString,
    pub args: Vec<OsString>,
}

struct Script {
    replies: VecDeque<Option<Output>>,
    invocations: Vec<Invocation>,
}

thread_local! {
    static SCRIPT: RefCell<Option<Script>> = const { RefCell::new(None) };
}

struct Reset;

impl Drop for Reset {
    fn drop(&mut self) {
        SCRIPT.with(|script| *script.borrow_mut() = None);
    }
}

/// Unconfigured tests see an unavailable window service. Configured tests
/// must supply every reply, so unexpected commands fail without spawning.
pub(super) fn run(command: &Command) -> Option<Output> {
    SCRIPT.with(|script| {
        let mut script = script.borrow_mut();
        let script = script.as_mut()?;
        script.invocations.push(Invocation {
            program: command.get_program().to_owned(),
            args: command.get_args().map(ToOwned::to_owned).collect(),
        });
        script
            .replies
            .pop_front()
            .expect("window command has no scripted reply")
    })
}

pub(super) fn capture<T>(
    replies: impl IntoIterator<Item = Option<Output>>,
    action: impl FnOnce() -> T,
) -> (T, Vec<Invocation>) {
    SCRIPT.with(|script| {
        let mut script = script.borrow_mut();
        assert!(script.is_none(), "nested window-command fixture");
        *script = Some(Script {
            replies: replies.into_iter().collect(),
            invocations: Vec::new(),
        });
    });
    let _reset = Reset;
    let result = action();
    let script = SCRIPT.with(|script| script.borrow_mut().take().unwrap());
    assert!(script.replies.is_empty(), "unused window-command replies");
    (result, script.invocations)
}
