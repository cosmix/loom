//! Running each runner's selection and judging what it printed.

use anyhow::{bail, Result};
use std::collections::BTreeSet;
use std::path::Path;

use super::{ImpactOutcome, FULL_SUITE, IMPACT_TIMEOUT};
use crate::skills::project::declares_dependencies;
use crate::testrun::{classify, RunOutcome, RunOutput, TestRunnerAdapter, TestTarget};
use crate::verify::criteria::{ProbeRun, ProbeRunner};

/// A command longer than this many bytes displays shortened.
const COMMAND_DISPLAY_CHARS: usize = 200;

/// The most test names a failure lists.
const SELECTED_NAMES_SHOWN: usize = 10;

/// The selections of one completion check and what they left.
pub(super) struct Selection<'a> {
    /// The checkout package directories are relative to.
    root: &'a Path,
    runner: &'a dyn ProbeRunner,
    ran: Vec<String>,
    notes: BTreeSet<String>,
    failures: Vec<String>,
}

impl<'a> Selection<'a> {
    pub(super) fn new(
        root: &'a Path,
        runner: &'a dyn ProbeRunner,
        notes: BTreeSet<String>,
    ) -> Self {
        Self {
            root,
            runner,
            ran: Vec::new(),
            notes,
            failures: Vec::new(),
        }
    }

    /// Run `adapter`'s selection of `targets` in `package` and judge it. A
    /// selection that cannot be run (the command does not spawn, for one too long
    /// for the OS to exec) is a note, never a failure.
    pub(super) fn run_group(
        &mut self,
        package: &Path,
        adapter: &dyn TestRunnerAdapter,
        targets: &[TestTarget],
    ) {
        let package_dir = self.root.join(package);
        if let Some(missing) = missing_environment(&package_dir, self.root, adapter) {
            let name = package_label(package);
            let note = format!(
                "`{name}` has no {missing} in this worktree (a plan `provision` entry \
                 installs it); {FULL_SUITE}"
            );
            self.notes.insert(note);
            return;
        }
        let Some(command) = adapter.select_command(targets, &package_dir) else {
            let runner = adapter.name();
            let note = format!("{runner} cannot select tests by file; {FULL_SUITE}");
            self.notes.insert(note);
            return;
        };
        let shown = shown_command(&command, targets.len());
        // Only the root cause is shown: the context chain repeats the whole command.
        let run = match self.runner.run(&command, &package_dir) {
            Ok(run) => run,
            Err(error) => {
                let cause = error.root_cause();
                let note = format!("`{shown}` could not be run ({cause}); {FULL_SUITE}");
                self.notes.insert(note);
                return;
            }
        };
        if run.timed_out {
            let secs = IMPACT_TIMEOUT.as_secs();
            let note = format!("`{shown}` timed out after {secs} s; {FULL_SUITE}");
            self.notes.insert(note);
            return;
        }
        self.judge(shown, adapter, &run, targets);
    }

    /// Record a finished run, shown as `command`, as passed, a note, or a failure
    /// naming its tests.
    fn judge(
        &mut self,
        command: String,
        adapter: &dyn TestRunnerAdapter,
        run: &ProbeRun,
        targets: &[TestTarget],
    ) {
        let output = RunOutput {
            stdout: &run.stdout,
            stderr: &run.stderr,
            exit_code: run.exit_code,
        };
        if run.exit_code == Some(127) {
            let note = format!("`{command}` could not start (exit 127); {FULL_SUITE}");
            self.notes.insert(note);
            return;
        }
        let tests = selected(targets);
        let runner = adapter.name();
        match classify(&adapter.parse(&output), run.exit_code) {
            RunOutcome::Passed => self.ran.push(command),
            RunOutcome::NotSelected => {
                let note = format!("`{command}` selected no test; {FULL_SUITE}");
                self.notes.insert(note);
            }
            RunOutcome::Failed => self.failures.push(format!("`{command}` fails ({tests})")),
            RunOutcome::BuildFailed => {
                let failure = format!("`{command}` does not build ({tests})");
                self.failures.push(failure);
            }
            RunOutcome::Unparsed if run.exit_code == Some(0) => {
                let note = format!(
                    "`{command}` passed on its exit code alone ({runner} output not parsed)"
                );
                self.notes.insert(note);
                self.ran.push(command);
            }
            RunOutcome::Unparsed => {
                let exit = run
                    .exit_code
                    .map_or_else(|| "a signal".to_string(), |code| format!("exit {code}"));
                let failure =
                    format!("`{command}` ended with {exit} ({runner} output not parsed; {tests})");
                self.failures.push(failure);
            }
        }
    }

    /// The outcome, or an error listing every failed selection.
    pub(super) fn finish(self, stage_id: &str) -> Result<ImpactOutcome> {
        if self.failures.is_empty() {
            return Ok(ImpactOutcome {
                ran: self.ran,
                notes: self.notes.into_iter().collect(),
            });
        }
        bail!(
            "impact-selected tests failed for stage '{stage_id}':\n  - {}\n\
             These tests reach code the stage changed: fix the regression, then complete the \
             stage again.",
            self.failures.join("\n  - ")
        )
    }
}

/// The package as a note names it: its checkout-relative path, `.` for the root.
fn package_label(package: &Path) -> String {
    if package.as_os_str().is_empty() {
        ".".to_string()
    } else {
        package.display().to_string()
    }
}

/// The installed environment `adapter`'s run in `package_dir` needs and lacks,
/// there and in every ancestor up to and including `root`. The confined runner
/// denies the network, so a missing one cannot be built and the run proves
/// nothing about the change. A JS package needs `node_modules` only when it
/// declares dependencies (an unreadable or unparsable manifest counts as
/// declaring them); a Python package with a `uv.lock`, run by `uv run`, needs
/// its `.venv`.
fn missing_environment(
    package_dir: &Path,
    root: &Path,
    adapter: &dyn TestRunnerAdapter,
) -> Option<&'static str> {
    let needed = match adapter.language() {
        "javascript" if declares_dependencies(root, package_dir) => "node_modules",
        "python" if package_dir.join("uv.lock").is_file() => ".venv",
        _ => return None,
    };
    let present = package_dir
        .ancestors()
        .take_while(|dir| dir.starts_with(root))
        .any(|dir| dir.join(needed).is_dir());
    (!present).then_some(needed)
}

/// The command as a note shows it: a long one cut to its first
/// `COMMAND_DISPLAY_CHARS` bytes and the number of targets it selects.
fn shown_command(command: &str, target_count: usize) -> String {
    if command.len() <= COMMAND_DISPLAY_CHARS {
        return command.to_string();
    }
    let end = command.floor_char_boundary(COMMAND_DISPLAY_CHARS);
    format!("{}… ({target_count} targets)", &command[..end])
}

/// The selected tests, as a failure names them: the first `SELECTED_NAMES_SHOWN`.
fn selected(targets: &[TestTarget]) -> String {
    let names: Vec<&str> = targets
        .iter()
        .map(|target| target.name.as_deref().unwrap_or(&target.file))
        .collect();
    let shown = names[..names.len().min(SELECTED_NAMES_SHOWN)].join(", ");
    match names.len().checked_sub(SELECTED_NAMES_SHOWN) {
        Some(more) if more > 0 => format!("selected: {shown}, and {more} more"),
        _ => format!("selected: {shown}"),
    }
}

#[cfg(test)]
#[path = "runs_tests.rs"]
mod tests;
