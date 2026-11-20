//! The registered adapters, and finding the one a command runs.

use std::path::Path;

use super::recognize::invocations;
use super::{adapters, RunOutput, RunningTests, TestRunnerAdapter};

/// Every registered adapter, in registration order.
pub fn all() -> &'static [&'static dyn TestRunnerAdapter] {
    adapters::ALL
}

/// The adapter called `name` (`cargo-test`).
pub fn by_name(name: &str) -> Option<&'static dyn TestRunnerAdapter> {
    all().iter().copied().find(|adapter| adapter.name() == name)
}

/// The first adapter recognising a simple command of `command` (lexed with
/// `shell_lex`), taking the simple commands in order, including package-script
/// indirection (`npm test` runs `scripts.test` of `package.json`). `cwd` is the
/// directory `command` runs in.
pub fn recognize(command: &str, cwd: &Path) -> Option<&'static dyn TestRunnerAdapter> {
    invocations(command, cwd)
        .iter()
        .find_map(|argv| all().iter().copied().find(|a| a.recognizes(argv)))
}

/// The tests a stopped run of `command` in `cwd` left unfinished, read by the
/// adapter that recognises `command`, else by the first adapter that can read
/// them from `out` (a wrapper script that runs `cargo test`). When no adapter
/// can read them, [`RunningTests::Unknown`] if an adapter recognises
/// `command` as a test run, else `None`: a build or a script is no test run.
pub fn unfinished_tests(command: &str, cwd: &Path, out: &RunOutput<'_>) -> Option<RunningTests> {
    let recognised = recognize(command, cwd);
    let named = recognised
        .into_iter()
        .chain(all().iter().copied())
        .find_map(|adapter| adapter.unfinished_tests(out));
    match named {
        Some(names) => Some(RunningTests::Named(names)),
        None => recognised.map(|_| RunningTests::Unknown),
    }
}
