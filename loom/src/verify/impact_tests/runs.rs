//! Running each runner's selection and judging what it printed.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use super::failure::{failure_id, output_tail, package_label, FailureKind};
use super::scoped;
use super::{FULL_SUITE, IMPACT_TIMEOUT};
use crate::testrun::{classify, RunOutcome, RunOutput, TestRunnerAdapter, TestTarget};
use crate::verify::criteria::{ProbeRun, ProbeRunner};
use crate::verify::environment::run_output_denial;

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
    Inconclusive(Inconclusive),
    Failed(Failure),
}

/// Why a run proved nothing.
pub(super) struct Inconclusive {
    why: String,
    environment: bool,
    /// The command the sandbox refused, when one ran: [`Self::note`] names it,
    /// so two selections refused alike keep a note each.
    command: Option<String>,
}

impl Inconclusive {
    /// The sandbox refused `command`: `denial` is the line it printed
    /// (`environment::sandbox_denial`).
    pub(super) fn environment(denial: &str, command: &str) -> Self {
        let mut refused =
            Self::unrunnable(format!("environment failure (sandbox denial: {denial})"));
        refused.command = Some(command.to_string());
        refused
    }

    /// Why, naming the command the sandbox refused when there was one.
    pub(super) fn note(&self) -> String {
        match &self.command {
            Some(command) => format!("{} running `{command}`", self.why),
            None => self.why.clone(),
        }
    }

    /// The checkout lacks an installed environment the runner needs: `why`
    /// says which.
    pub(super) fn missing_dependency(why: String) -> Self {
        Self::unrunnable(why)
    }

    /// A run the checkout's environment cannot do, not a verdict on the code:
    /// a refused sandbox or a missing dependency. [`Self::is_environment`] is
    /// true for it, so a caller can tell it from a run that proved nothing for
    /// another reason.
    fn unrunnable(why: String) -> Self {
        Self {
            why,
            environment: true,
            command: None,
        }
    }

    /// Whether the run could not happen for want of its environment: a refused
    /// sandbox or a missing dependency.
    pub(super) fn is_environment(&self) -> bool {
        self.environment
    }
}

impl From<String> for Inconclusive {
    fn from(why: String) -> Self {
        Self {
            why,
            environment: false,
            command: None,
        }
    }
}

impl fmt::Display for Inconclusive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.why)
    }
}

/// A selection run that failed.
pub(super) struct Failure {
    /// The command, as the gate shows it.
    pub(super) command: String,
    pub(super) kind: FailureKind,
    /// The tests the run's output names failed ([`TestRunnerAdapter::failed_tests`]);
    /// empty when the runner's output does not name them.
    pub(super) failed_tests: Vec<String>,
    pub(super) output: String,
    /// The loopback opt-in note ([`ProbeRun::bind_note`]), read from the run's
    /// whole output before `output` is cut.
    pub(super) note: Option<String>,
}

impl Site<'_> {
    /// Run `adapter`'s selection of `targets` in `package` and judge it; a
    /// runner selecting by libtest path runs one group per cargo target
    /// (`scoped`). A selection that cannot be run (the command does not
    /// spawn, for one too long for the OS to exec) is inconclusive, never a
    /// failure.
    pub(super) fn run_selection(
        &self,
        package: &Path,
        adapter: &dyn TestRunnerAdapter,
        targets: &[TestTarget],
    ) -> Run {
        let package_dir = self.root.join(package);
        if let Some(missing) = missing_environment(&package_dir, self.root, adapter) {
            let name = package_label(package);
            let place = self.place;
            let why = format!("`{name}` has no {missing} in {place}");
            return Run::Inconclusive(Inconclusive::missing_dependency(why));
        }
        let Some(command) = scoped::select_command(self.root, package, adapter, targets) else {
            let runner = adapter.name();
            return Run::Inconclusive(format!("{runner} cannot select tests by file").into());
        };
        let shown = shown_command(&command, targets.len());
        // Only the root cause is shown: the context chain repeats the whole command.
        let run = match self.runner.run(&command, &package_dir) {
            Ok(run) => run,
            Err(error) => {
                let cause = error.root_cause();
                return Run::Inconclusive(format!("`{shown}` could not be run ({cause})").into());
            }
        };
        if run.timed_out {
            let secs = IMPACT_TIMEOUT.as_secs();
            return Run::Inconclusive(format!("`{shown}` timed out after {secs} s").into());
        }
        judge(shown, adapter, &run)
    }
}

/// A finished run, shown as `command`: passed, inconclusive, or how it failed.
fn judge(command: String, adapter: &dyn TestRunnerAdapter, run: &ProbeRun) -> Run {
    if run.exit_code == Some(127) {
        return Run::Inconclusive(format!("`{command}` could not start (exit 127)").into());
    }
    if !run.output_complete {
        return judge_incomplete_capture(command, adapter, run);
    }
    // A certified cache hit keeps only output tails; its recorded count decides.
    if let Some(verdict) = judge_cached(&command, adapter, run) {
        return verdict;
    }
    judge_parsed(command, adapter, run)
}

/// A finished, completely captured run judged by what the adapter parses from
/// its output. A run the adapter reads nothing from that failed and printed a
/// sandbox denial is inconclusive: the sandbox, not the tests, ended it.
fn judge_parsed(command: String, adapter: &dyn TestRunnerAdapter, run: &ProbeRun) -> Run {
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
            return Run::Inconclusive(format!("`{command}` selected no test").into());
        }
        RunOutcome::Unparsed if run.exit_code == Some(0) => {
            let note = exit_code_note(&command, adapter);
            return Run::Passed {
                command,
                note: Some(note),
            };
        }
        RunOutcome::Failed => FailureKind::Fails {
            failed: summary.failed,
        },
        RunOutcome::BuildFailed => FailureKind::DoesNotBuild,
        RunOutcome::Unparsed => {
            if let Some(denial) = run_output_denial(&run.stdout, &run.stderr) {
                return Run::Inconclusive(Inconclusive::environment(&denial, &command));
            }
            FailureKind::Unparsed {
                exit_code: run.exit_code,
            }
        }
    };
    Run::Failed(Failure {
        command,
        kind,
        failed_tests: failed_tests(adapter, run),
        output: output_tail(run),
        note: run.bind_note(),
    })
}

/// A run whose capture lost output. Only a passing exit needs the output its
/// count would come from. A failing exit fails whatever the capture lost, even
/// when the kept tail prints a denial marker: a truncated tail cannot tell a
/// refused sandbox from a test's own assertion text. The kept output still
/// names the failed tests it shows.
fn judge_incomplete_capture(
    command: String,
    adapter: &dyn TestRunnerAdapter,
    run: &ProbeRun,
) -> Run {
    if run.exit_code == Some(0) {
        return Run::Inconclusive(format!("`{command}` output capture was incomplete").into());
    }
    Run::Failed(Failure {
        command,
        kind: FailureKind::Unparsed {
            exit_code: run.exit_code,
        },
        failed_tests: failed_tests(adapter, run),
        output: output_tail(run),
        note: run.bind_note(),
    })
}

/// The tests `run`'s whole output names failed, read before the output is cut
/// to its recorded tail.
fn failed_tests(adapter: &dyn TestRunnerAdapter, run: &ProbeRun) -> Vec<String> {
    let output = RunOutput {
        stdout: &run.stdout,
        stderr: &run.stderr,
        exit_code: run.exit_code,
    };
    adapter.failed_tests(&output).unwrap_or_default()
}

/// The verdict a certified cache hit's recorded count gives; `None` leaves the
/// run to the parse path.
fn judge_cached(command: &str, adapter: &dyn TestRunnerAdapter, run: &ProbeRun) -> Option<Run> {
    let cached = run.cached.as_ref()?;
    match (cached.tests_executed, run.exit_code) {
        (Some(0), _) => Some(Run::Inconclusive(
            format!("`{command}` selected no test").into(),
        )),
        (Some(_), Some(0)) => Some(Run::Passed {
            command: command.to_string(),
            note: None,
        }),
        (None, Some(0)) => Some(Run::Passed {
            command: command.to_string(),
            note: Some(exit_code_note(command, adapter)),
        }),
        _ => None,
    }
}

/// The note a pass on its exit code alone carries when no output was parsed.
fn exit_code_note(command: &str, adapter: &dyn TestRunnerAdapter) -> String {
    let runner = adapter.name();
    format!("`{command}` passed on its exit code alone ({runner} output not parsed)")
}

/// A selection that failed in the worktree, with what re-running it needs.
pub(in crate::verify) struct Failed {
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
pub(in crate::verify) struct Selection<'a> {
    site: Site<'a>,
    pub(in crate::verify) ran: Vec<String>,
    pub(in crate::verify) notes: BTreeSet<String>,
    pub(in crate::verify) failed: Vec<Failed>,
}

impl<'a> Selection<'a> {
    /// Selections run in the worktree checked out at `root`.
    pub(in crate::verify) fn new(
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
    pub(in crate::verify) fn run_group(
        &mut self,
        package: &Path,
        adapter: &'static dyn TestRunnerAdapter,
        targets: &[TestTarget],
    ) {
        match self.site.run_selection(package, adapter, targets) {
            Run::Passed { command, note } => {
                self.notes.extend(note);
                self.ran.push(command);
            }
            Run::Inconclusive(why) => {
                self.notes.insert(format!("{}; {FULL_SUITE}", why.note()));
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
