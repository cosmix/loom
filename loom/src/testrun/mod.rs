//! Test-runner adapters.
//!
//! An adapter knows one test runner: it recognises the runner's command line,
//! builds the commands that run one test or a selection, and reads a run's
//! output into a [`RunSummary`] that [`classify`] turns into a [`RunOutcome`].
//! Each adapter lives in `adapters/`; [`registry`] lists them and finds the one
//! a shell command invokes. [`recognize`] holds the argv and output helpers the
//! adapters share; [`command`] the quoting and selection helpers their
//! commands are built with.

use std::path::Path;

mod adapters;
pub mod command;
#[cfg(test)]
mod fixture_support;
pub mod languages;
mod outcome;
pub mod recognize;
pub mod registry;
#[cfg(test)]
mod tests;

pub use outcome::classify;

/// The captured output of one test command.
#[derive(Debug, Clone, Copy)]
pub struct RunOutput<'a> {
    pub stdout: &'a str,
    pub stderr: &'a str,
    /// `None` when the process ended without an exit code (killed by a signal).
    pub exit_code: Option<i32>,
}

/// What an adapter read from a run's output. A `None` count means the output
/// did not report it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunSummary {
    /// Tests that actually ran (passed + failed).
    pub executed: Option<u64>,
    pub passed: Option<u64>,
    pub failed: Option<u64>,
    pub skipped: Option<u64>,
    /// Compilation or collection failed before tests ran.
    pub build_failed: bool,
}

/// The verdict on one run; see [`classify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Passed,
    Failed,
    BuildFailed,
    /// The filter matched no test: zero tests ran.
    NotSelected,
    /// The output said nothing the adapter could read; the exit code decides.
    Unparsed,
}

/// A test to select: the file declaring it and, when known, its name in the
/// form the adapter's runner filters by (DESIGN D5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestTarget {
    pub file: String,
    pub name: Option<String>,
}

/// One test runner. Each implementation is a unit struct exposed as
/// `pub static ADAPTER` from its module in `adapters/`.
pub trait TestRunnerAdapter: Sync {
    /// The adapter's name, e.g. `cargo-test`.
    fn name(&self) -> &'static str;

    /// The language profile the runner's tests are written in (DESIGN D6).
    fn language(&self) -> &'static str;

    /// Whether `argv`, the words of one simple command with any runner
    /// prefixes (`env`, `bunx`, `uv run`, ...) still in place, runs this runner.
    fn recognizes(&self, argv: &[String]) -> bool;

    /// Whether `argv` is recognised and runs the whole suite: no test-name,
    /// file or target filter.
    fn is_full_run(&self, argv: &[String]) -> bool;

    /// The shell command that runs test `test` declared in `file`, run with
    /// `package_dir` as its cwd.
    fn single_test_command(&self, file: &str, test: &str, package_dir: &Path) -> String;

    /// One shell command running every target, or `None` when the runner
    /// cannot select tests by file or name.
    fn select_command(&self, targets: &[TestTarget], package_dir: &Path) -> Option<String>;

    /// Read the counts and the build status from a run's output.
    fn parse(&self, out: &RunOutput<'_>) -> RunSummary;
}
