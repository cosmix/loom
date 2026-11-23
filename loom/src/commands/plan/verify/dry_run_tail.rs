//! The lines of a failed test run's output that name its failures, then its end, for the
//! dry run's note: the runner's own failure list and verdict, found without reading any
//! runner's format.

use std::path::Path;

use crate::models::dispute::strip_control;
use crate::testrun::recognize::strip_ansi;
use crate::verify::criteria::{has_test_summary, CriterionResult};

/// Most lines containing [`FAIL_MARK`] the excerpt keeps.
const MAX_FAIL_LINES: usize = 20;
/// Most non-empty lines of the end of the output the excerpt keeps.
const TAIL_LINES: usize = 6;
/// Most characters the excerpt holds, its trailing ellipsis included.
const MAX_CHARS: usize = 2_000;
/// What a runner prints on a failure line: `test x ... FAILED`, `test result: FAILED`,
/// pytest `FAILED path::test`, jest `FAIL file`, go `--- FAIL: TestX`.
const FAIL_MARK: &str = "FAIL";

/// The excerpt ([`tail_of`]) of the output of `result` that holds the test summary: stdout when
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

/// The excerpt of `text`: its first [`MAX_FAIL_LINES`] non-empty lines containing
/// [`FAIL_MARK`] (a multi-binary run keeps its failures mid-output), a line naming how many
/// more were left out, then the last [`TAIL_LINES`] non-empty lines (the verdict), a line
/// never repeated. At most [`MAX_CHARS`] characters, cut from the end and closed by `…`.
/// ANSI sequences and control characters other than newline are removed first: the output
/// is untrusted text a terminal will print.
pub(super) fn tail_of(text: &str) -> String {
    let clean = strip_control(&strip_ansi(text), &['\n']);
    let lines: Vec<&str> = clean
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    let failing: Vec<usize> = (0..lines.len())
        .filter(|&index| lines[index].contains(FAIL_MARK))
        .collect();
    let (shown, beyond) = failing.split_at(failing.len().min(MAX_FAIL_LINES));
    let tail_from = lines.len().saturating_sub(TAIL_LINES);
    // A failure line past the cap that lies in the tail window is printed there.
    let left_out = beyond.iter().filter(|&&index| index < tail_from).count();
    let mut excerpt: Vec<String> = shown
        .iter()
        .map(|&index| lines[index].to_string())
        .collect();
    if left_out > 0 {
        excerpt.push(format!(
            "… {left_out} more lines containing {FAIL_MARK} left out"
        ));
    }
    let tail = lines[tail_from..]
        .iter()
        .filter(|line| !shown.iter().any(|&index| lines[index] == **line));
    excerpt.extend(tail.map(|line| line.to_string()));
    capped(excerpt.join("\n"))
}

/// `text` cut to [`MAX_CHARS`] characters, the cut marked by a trailing `…`.
fn capped(text: String) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text;
    }
    let kept: String = text.chars().take(MAX_CHARS - 1).collect();
    format!("{kept}…")
}

#[cfg(test)]
#[path = "dry_run_tail_tests.rs"]
mod tests;
