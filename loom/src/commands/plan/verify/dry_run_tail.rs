//! The end of a failed test run's output, for the dry run's note: the runner's
//! own failure list and verdict, named without reading any runner's format.

use std::path::Path;

use crate::models::dispute::strip_control;
use crate::testrun::recognize::strip_ansi;
use crate::verify::criteria::{has_test_summary, CriterionResult};

/// Most non-empty lines the tail keeps.
const MAX_LINES: usize = 12;
/// Most characters the tail holds, its leading ellipsis included.
const MAX_CHARS: usize = 1_200;

/// The tail of the output of `result` that holds the test summary: stdout when
/// a runner's summary reads from it alone, else stderr when it does, else
/// stdout then stderr (a summary split over both).
pub(super) fn summary_tail(command: &str, dir: &Path, result: &CriterionResult) -> String {
    let stdout_only = CriterionResult {
        stderr: String::new(),
        ..result.clone()
    };
    let stderr_only = CriterionResult {
        stdout: String::new(),
        ..result.clone()
    };
    if has_test_summary(command, dir, &stdout_only) {
        tail_of(&result.stdout)
    } else if has_test_summary(command, dir, &stderr_only) {
        tail_of(&result.stderr)
    } else {
        tail_of(&format!("{}\n{}", result.stdout, result.stderr))
    }
}

/// The last [`MAX_LINES`] non-empty lines of `text`, at most [`MAX_CHARS`]
/// characters, cut from the front and led by `…` when anything was cut. ANSI
/// sequences and control characters other than newline are removed first: the
/// output is untrusted text a terminal will print.
pub(super) fn tail_of(text: &str) -> String {
    let clean = strip_control(&strip_ansi(text), &['\n']);
    let lines: Vec<&str> = clean
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    let dropped_lines = lines.len() > MAX_LINES;
    let joined = lines[lines.len().saturating_sub(MAX_LINES)..].join("\n");
    let length = joined.chars().count();
    if !dropped_lines && length <= MAX_CHARS {
        return joined;
    }
    let kept: String = joined
        .chars()
        .skip(length.saturating_sub(MAX_CHARS - 1))
        .collect();
    format!("…{kept}")
}

#[cfg(test)]
#[path = "dry_run_tail_tests.rs"]
mod tests;
