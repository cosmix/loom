//! A failed impact selection: the id the gate reports and a dispute names,
//! how it failed, and whether one failure is no worse than another.
//!
//! A selection is one runner's tests in one package, so a failure is judged
//! per selection: the adapters read counts from a run's output, never which
//! test failed.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::models::dispute::{
    strip_control, tail_to_byte_limit, truncate_to_byte_limit, FAILURE_OUTPUT_MAX_BYTES,
};
use crate::testrun::registry;
use crate::verify::criteria::ProbeRun;

/// How a selection failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum FailureKind {
    /// Tests ran and some failed; `failed` is the count the runner reported.
    Fails {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failed: Option<u64>,
    },
    /// The selection did not build.
    DoesNotBuild,
    /// The run ended non-zero, or by a signal, with output its adapter could
    /// not read.
    Unparsed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
}

impl FailureKind {
    /// Whether `later` is provably no worse than `self`: both runs counted
    /// their failed tests, and `later` failed no more. Every other pair, an
    /// identical one included, proves nothing: a build failure, output the
    /// adapter could not read or a missing count says nothing about whether
    /// the same thing broke. Only the base comparison asks it, of two runs
    /// the same gate made; an accepted record must match exactly
    /// (`accepted`), since a stage session reported it.
    pub fn covers(&self, later: &FailureKind) -> bool {
        matches!(
            (self, later),
            (Self::Fails { failed: Some(known) }, Self::Fails { failed: Some(now) })
                if now <= known
        )
    }

    /// The failure as the gate phrases it: `fails (2 failed)`.
    pub fn describe(&self) -> String {
        match self {
            Self::Fails { failed: Some(n) } => format!("fails ({n} failed)"),
            Self::Fails { failed: None } => "fails".to_string(),
            Self::DoesNotBuild => "does not build".to_string(),
            Self::Unparsed { exit_code } => {
                let exit =
                    exit_code.map_or_else(|| "a signal".to_string(), |c| format!("exit {c}"));
                format!("ends with {exit} (output not parsed)")
            }
        }
    }
}

/// One failed impact selection, as the gate reports it, the observed record
/// keeps it (`observed`) and a dispute copies it from that record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactFailure {
    /// `IT-<runner>:<package>` (`failure_id`).
    pub id: String,
    /// The selection command, as the gate shows it.
    pub command: String,
    /// The selected tests: libtest paths, or files for a runner selecting by
    /// file.
    pub tests: Vec<String>,
    pub kind: FailureKind,
    /// What the same selection did at the stage's base.
    pub base: String,
    /// How an earlier dispute accepted this selection's failure, when this
    /// run fails otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted: Option<String>,
    /// The end of the run's output, at most `FAILURE_OUTPUT_MAX_BYTES`.
    pub output: String,
}

impl ImpactFailure {
    /// The failure as the gate lists it.
    pub fn line(&self) -> String {
        let accepted = self
            .accepted
            .as_deref()
            .map(|why| format!("; {why}"))
            .unwrap_or_default();
        format!(
            "{}: `{}` {} ({}); {}{accepted}",
            self.id,
            self.command,
            self.kind.describe(),
            listed(&self.tests),
            self.base
        )
    }

    /// The failure with every field cut to its bound and stripped of control
    /// characters (the output keeps its line breaks and tabs): what loom
    /// records and a dispute carries. A selection of more than
    /// `MAX_RECORDED_TESTS` tests keeps the first ones, so an accepted record
    /// of it never matches the whole selection and judges it again.
    pub fn bounded(&self) -> Self {
        let text = |s: &str| truncate_to_byte_limit(&strip_control(s, &[]), MAX_TEXT_BYTES);
        Self {
            id: truncate_to_byte_limit(&self.id, MAX_ID_BYTES),
            command: text(&self.command),
            tests: self
                .tests
                .iter()
                .take(MAX_RECORDED_TESTS)
                .map(|test| truncate_to_byte_limit(&strip_control(test, &[]), MAX_TEST_BYTES))
                .collect(),
            kind: self.kind.clone(),
            base: text(&self.base),
            accepted: self.accepted.as_deref().map(text),
            output: tail_to_byte_limit(
                &strip_control(&self.output, &['\n', '\t']),
                FAILURE_OUTPUT_MAX_BYTES,
            ),
        }
    }
}

/// Most bytes of an id, a selected test's name, and the command, base and
/// accepted texts a recorded failure keeps.
const MAX_ID_BYTES: usize = 512;
const MAX_TEST_BYTES: usize = 512;
const MAX_TEXT_BYTES: usize = 1024;
/// Most selected tests a recorded failure keeps.
const MAX_RECORDED_TESTS: usize = 64;

/// What starts every id; the runner name follows it up to [`ID_SEPARATOR`].
const ID_PREFIX: &str = "IT-";
/// Ends the runner name: no registered runner name holds it (they are
/// lowercase words joined by `-`), so the first one splits an id whatever
/// its package path holds.
const ID_SEPARATOR: char = ':';

/// The id of `runner`'s selection in `package`, checkout-relative:
/// `IT-<runner>:<package>`.
pub fn failure_id(runner: &str, package: &Path) -> String {
    format!(
        "{ID_PREFIX}{runner}{ID_SEPARATOR}{}",
        package_label(package)
    )
}

/// Check that `id` has the shape `failure_id` gives: a registered runner
/// and a package path of printable characters, at most `MAX_ID_BYTES` long.
pub fn check_failure_id(id: &str) -> Result<()> {
    let shape = "IT-<runner>:<package>, as `loom stage complete` lists it";
    let Some((runner, package)) = id
        .strip_prefix(ID_PREFIX)
        .and_then(|rest| rest.split_once(ID_SEPARATOR))
    else {
        bail!("'{id}' is not an impact-test id ({shape})");
    };
    if registry::by_name(runner).is_none() {
        bail!("'{id}' names no test runner loom knows ({shape})");
    }
    if package.is_empty() || id.len() > MAX_ID_BYTES || id.chars().any(char::is_control) {
        bail!("'{id}' is not an impact-test id ({shape})");
    }
    Ok(())
}

/// The package as a note names it: its checkout-relative path, `.` for the root.
pub(super) fn package_label(package: &Path) -> String {
    if package.as_os_str().is_empty() {
        ".".to_string()
    } else {
        package.display().to_string()
    }
}

/// The most test names a failure lists.
const SELECTED_NAMES_SHOWN: usize = 10;

/// The selected tests, as a failure names them: the first `SELECTED_NAMES_SHOWN`.
pub(super) fn listed(tests: &[String]) -> String {
    let shown = tests[..tests.len().min(SELECTED_NAMES_SHOWN)].join(", ");
    match tests.len().checked_sub(SELECTED_NAMES_SHOWN) {
        Some(more) if more > 0 => format!("selected: {shown}, and {more} more"),
        _ => format!("selected: {shown}"),
    }
}

/// The end of what `run` printed, stdout first: a runner's verdict and its
/// failures come last.
pub(super) fn output_tail(run: &ProbeRun) -> String {
    let combined = match (run.stdout.trim().is_empty(), run.stderr.trim().is_empty()) {
        (_, true) => run.stdout.clone(),
        (true, false) => run.stderr.clone(),
        (false, false) => format!("{}\n--- stderr ---\n{}", run.stdout, run.stderr),
    };
    tail_to_byte_limit(&combined, FAILURE_OUTPUT_MAX_BYTES)
}
