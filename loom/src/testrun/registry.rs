//! The registered adapters, and finding the one a command runs.

use std::path::Path;

use super::recognize::invocations;
use super::{adapters, TestRunnerAdapter};

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
