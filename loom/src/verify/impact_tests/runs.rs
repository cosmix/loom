//! Running each runner's selection and judging what it printed.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::failure::{failure_id, output_tail, package_label, FailureKind};
use super::{FULL_SUITE, IMPACT_TIMEOUT};
use crate::testrun::{classify, RunOutcome, RunOutput, TestRunnerAdapter, TestTarget};
use crate::verify::criteria::{ProbeRun, ProbeRunner};

/// A command longer than this many bytes displays shortened.
const COMMAND_DISPLAY_CHARS: usize = 200;

/// A checkout selections run in, and the runner for it.
pub(super) struct Site<'a> {
    /// The checkout package directories are relative to.
    pub(super) root: &'a Path,
    pub(super) runner: &'a dyn ProbeRunner,
    /// How a note names the checkout, with how an installed environment gets
    /// there.
    pub(super) place: &'static str,
}

/// How one run of a selection ended.
pub(super) enum Run {
    /// It passed; `note` when only its exit code says so.
    Passed {
        command: String,
        note: Option<String>,
    },
    /// It was not run, or proved nothing: why.
    Inconclusive(String),
    Failed(Failure),
}

/// A selection run that failed.
pub(super) struct Failure {
    /// The command, as the gate shows it.
    pub(super) command: String,
    pub(super) kind: FailureKind,
    pub(super) output: String,
}

impl Site<'_> {
    /// Run `adapter`'s selection of `targets` in `package` and judge it. A
    /// selection that cannot be run (the command does not spawn, for one too
    /// long for the OS to exec) is inconclusive, never a failure.
    pub(super) fn run(
        &self,
        package: &Path,
        adapter: &dyn TestRunnerAdapter,
        targets: &[TestTarget],
    ) -> Run {
        let package_dir = self.root.join(package);
        if let Some(missing) = missing_environment(&package_dir, self.root, adapter) {
            let name = package_label(package);
            let place = self.place;
            return Run::Inconclusive(format!("`{name}` has no {missing} in {place}"));
        }
        let Some(command) = adapter.select_command(targets, &package_dir) else {
            let runner = adapter.name();
            return Run::Inconclusive(format!("{runner} cannot select tests by file"));
        };
        let shown = shown_command(&command, targets.len());
        // Only the root cause is shown: the context chain repeats the whole command.
        let run = match self.runner.run(&command, &package_dir) {
            Ok(run) => run,
            Err(error) => {
                let cause = error.root_cause();
                return Run::Inconclusive(format!("`{shown}` could not be run ({cause})"));
            }
        };
        if run.timed_out {
            let secs = IMPACT_TIMEOUT.as_secs();
            return Run::Inconclusive(format!("`{shown}` timed out after {secs} s"));
        }
        judge(shown, adapter, &run)
    }
}

/// A finished run, shown as `command`: passed, inconclusive, or how it failed.
fn judge(command: String, adapter: &dyn TestRunnerAdapter, run: &ProbeRun) -> Run {
    if run.exit_code == Some(127) {
        return Run::Inconclusive(format!("`{command}` could not start (exit 127)"));
    }
    let output = RunOutput {
        stdout: &run.stdout,
        stderr: &run.stderr,
        exit_code: run.exit_code,
    };
    let summary = adapter.parse(&output);
    let kind = match classify(&summary, run.exit_code) {
        RunOutcome::Passed => {
            return Run::Passed {
                command,
                note: None,
            }
        }
        RunOutcome::NotSelected => {
            return Run::Inconclusive(format!("`{command}` selected no test"));
        }
        RunOutcome::Unparsed if run.exit_code == Some(0) => {
            let runner = adapter.name();
            let note =
                format!("`{command}` passed on its exit code alone ({runner} output not parsed)");
            return Run::Passed {
                command,
                note: Some(note),
            };
        }
        RunOutcome::Failed => FailureKind::Fails {
            failed: summary.failed,
        },
        RunOutcome::BuildFailed => FailureKind::DoesNotBuild,
        RunOutcome::Unparsed => FailureKind::Unparsed {
            exit_code: run.exit_code,
        },
    };
    Run::Failed(Failure {
        command,
        kind,
        output: output_tail(run),
    })
}

/// A selection that failed in the worktree, with what re-running it needs.
pub(super) struct Failed {
    pub(super) id: String,
    pub(super) package: PathBuf,
    pub(super) adapter: &'static dyn TestRunnerAdapter,
    pub(super) targets: Vec<TestTarget>,
    pub(super) failure: Failure,
    /// How an earlier dispute accepted this selection, when this run is worse.
    pub(super) accepted: Option<String>,
}

impl Failed {
    /// The selected tests, as a failure and an accepted record name them.
    pub(super) fn tests(&self) -> Vec<String> {
        labels(&self.targets)
    }
}

/// The selections of one completion check and what they left.
pub(super) struct Selection<'a> {
    site: Site<'a>,
    pub(super) ran: Vec<String>,
    pub(super) notes: BTreeSet<String>,
    pub(super) failed: Vec<Failed>,
}

impl<'a> Selection<'a> {
    /// Selections run in the worktree checked out at `root`.
    pub(super) fn new(
        root: &'a Path,
        runner: &'a dyn ProbeRunner,
        notes: BTreeSet<String>,
    ) -> Self {
        Self {
            site: Site {
                root,
                runner,
                place: "this worktree (a plan `provision` entry installs it)",
            },
            ran: Vec::new(),
            notes,
            failed: Vec::new(),
        }
    }

    /// Run `adapter`'s selection of `targets` in `package`, recording it as
    /// passed, a note, or a failure.
    pub(super) fn run_group(
        &mut self,
        package: &Path,
        adapter: &'static dyn TestRunnerAdapter,
        targets: &[TestTarget],
    ) {
        match self.site.run(package, adapter, targets) {
            Run::Passed { command, note } => {
                self.notes.extend(note);
                self.ran.push(command);
            }
            Run::Inconclusive(why) => {
                self.notes.insert(format!("{why}; {FULL_SUITE}"));
            }
            Run::Failed(failure) => self.failed.push(Failed {
                id: failure_id(adapter.name(), package),
                package: package.to_path_buf(),
                adapter,
                targets: targets.to_vec(),
                failure,
                accepted: None,
            }),
        }
    }
}

/// The installed environment `adapter` declares for a run in `package_dir`
/// ([`TestRunnerAdapter::required_environment`]) when it is missing there and
/// in every ancestor up to and including `root`.
fn missing_environment(
    package_dir: &Path,
    root: &Path,
    adapter: &dyn TestRunnerAdapter,
) -> Option<&'static str> {
    let needed = adapter.required_environment(root, package_dir)?;
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

/// Each target as a failure names it: its libtest path, or its file.
fn labels(targets: &[TestTarget]) -> Vec<String> {
    targets
        .iter()
        .map(|target| target.name.clone().unwrap_or_else(|| target.file.clone()))
        .collect()
}

#[cfg(test)]
#[path = "runs_tests.rs"]
mod tests;
