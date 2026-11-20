//! What libtest's console output says about tests still running when a run
//! was stopped.
//!
//! In its default (pretty) format, running in parallel, libtest prints a
//! test's line only when it finishes, and `test <name> has been running for
//! over 60 seconds` for one still running after a minute. Running one test at
//! a time, it prints `test <name> ... ` when a test starts and its result when
//! it ends. The terse format (`cargo test -q`) prints one character per
//! finished test, so a slow test that later finished cannot be told from one
//! still running (`fixtures/cargo-test/killed/terse.stdout`).

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::recognize::{pattern, strip_ansi};
use super::unfinished::names;

static RUNNING: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^running \d+ tests?$"));
static RESULT: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^test result: "));
static SLOW: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^test (.+) has been running for over \d+ seconds$"));
static FINISHED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^test (.+?)(?: - should panic(?: with .*)?)? \.\.\. (?:ok|FAILED|ignored)")
});
static STARTED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"^test (.+?)(?: - should panic(?: with .*)?)? \.\.\. ?$"));

/// The tests of the last test binary that started and did not finish, read
/// from stdout, else from stderr. `None` when neither shows a libtest run
/// whose running tests can be told: one in the terse format, one with no
/// pretty result line yet, and one whose last binary printed its `test
/// result:` line (what ran after it is what was stopped). An empty list means
/// the run shows no test still running: a test that ran for under a minute in
/// parallel is not named.
pub(super) fn unfinished(stdout: &str, stderr: &str) -> Option<Vec<String>> {
    unfinished_in(&strip_ansi(stdout)).or_else(|| unfinished_in(&strip_ansi(stderr)))
}

fn unfinished_in(text: &str) -> Option<Vec<String>> {
    let start = RUNNING.find_iter(text).last()?.end();
    let block = &text[start..];
    if RESULT.is_match(block) {
        return None;
    }
    let finished: HashSet<&str> = names(&FINISHED, block).collect();
    let started: Vec<&str> = block
        .lines()
        .last()
        .into_iter()
        .flat_map(|line| names(&STARTED, line))
        .collect();
    if finished.is_empty() && started.is_empty() {
        return None;
    }
    let mut running: Vec<String> = Vec::new();
    for name in names(&SLOW, block).chain(started) {
        if !finished.contains(name) && !running.iter().any(|seen| seen == name) {
            running.push(name.to_string());
        }
    }
    Some(running)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slow_test_without_a_result_is_still_running() {
        let stdout = "running 2 tests\n\
            test a::finishes has been running for over 60 seconds\n\
            test a::hangs has been running for over 60 seconds\n\
            test a::finishes ... ok\n";
        assert_eq!(unfinished(stdout, ""), Some(vec!["a::hangs".to_string()]));
    }

    #[test]
    fn only_the_last_binary_counts_and_a_sequential_start_is_read() {
        let stdout = "running 1 test\n\
            test shared::name has been running for over 60 seconds\n\
            test result: ok. 0 passed; 0 failed\n\
            running 2 tests\n\
            test early ... ok\n\
            test late - should panic ... ";
        assert_eq!(unfinished(stdout, ""), Some(vec!["late".to_string()]));
    }

    #[test]
    fn output_without_a_readable_libtest_run_says_nothing() {
        assert_eq!(unfinished("   Compiling loom v0.1.0\n", "error: x"), None);
        let ansi = "\u{1b}[32mrunning 1 test\u{1b}[0m\n";
        assert_eq!(unfinished("", ansi), None, "no result line yet");
        let only_slow = "running 1 test\ntest a has been running for over 60 seconds\n";
        assert_eq!(unfinished(only_slow, ""), None);
    }

    /// `make check` that ran `cargo test` to its end and then a runner that hung.
    #[test]
    fn a_finished_binary_followed_by_another_runner_cannot_tell() {
        let stdout = "running 2 tests\n\
            test a ... ok\n\
            test b ... ok\n\
            \n\
            test result: ok. 2 passed; 0 failed; 0 ignored\n\
            \n\
            === RUN   TestHang\n";
        assert_eq!(unfinished(stdout, ""), None);
    }
}
