//! Which tests a finished run's console output names as failed, for the
//! runners whose default output names each failed test on a line of its own.
//! Each reader returns `None` unless the output shows its own runner's
//! failure lines, so a runner loom cannot read here is told apart from one
//! whose run failed no named test. The recorded failing runs each reader was
//! written from are the `one-fail` and `suite` scenarios under
//! `fixtures/<adapter>/`.

use std::sync::LazyLock;

use regex::Regex;

use super::recognize::{pattern, strip_ansi};
use super::unfinished::names;
use super::RunOutput;

static LIBTEST_FAILED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(r"(?m)^test (.+?)(?: - should panic(?: with .*)?)? \.\.\. FAILED\s*$")
});
static NEXTEST_FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^\s*(?:FAIL|SIGSEGV|SIGABRT|TIMEOUT) \[\s*[\d.]+s\] (.+?)\s*$"));
static GO_FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^\s*--- FAIL: (\S+) \("));
static PYTEST_FAILED: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^(?:FAILED|ERROR) (\S+?)(?: - .*)?\s*$"));
static VITEST_FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^ FAIL  (.+ > .+?)\s*$"));
static JEST_FAILED: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^  ● (.+?)\s*$"));

/// `reader` applied to stdout and stderr together, ANSI codes stripped: some
/// runners print their failure lines on stderr (nextest, jest, vitest).
pub(super) fn read_both(
    out: &RunOutput<'_>,
    reader: fn(&str) -> Option<Vec<String>>,
) -> Option<Vec<String>> {
    let text = format!("{}\n{}", strip_ansi(out.stdout), strip_ansi(out.stderr));
    reader(&text)
}

/// libtest (`cargo test`): `test <name> ... FAILED` for each failed test.
pub(super) fn libtest(text: &str) -> Option<Vec<String>> {
    distinct(&LIBTEST_FAILED, text)
}

/// `cargo nextest run`: `FAIL [<time>] <binary> <name>`, repeated in the
/// closing summary; a crash or timeout prints its own status word.
pub(super) fn nextest(text: &str) -> Option<Vec<String>> {
    distinct(&NEXTEST_FAILED, text)
}

/// `go test`: `--- FAIL: <name> (<time>)`, indented for a subtest.
pub(super) fn go(text: &str) -> Option<Vec<String>> {
    distinct(&GO_FAILED, text)
}

/// pytest's short test summary: `FAILED <node id> - <message>`, and `ERROR`
/// for a test whose setup failed.
pub(super) fn pytest(text: &str) -> Option<Vec<String>> {
    distinct(&PYTEST_FAILED, text)
}

/// vitest: ` FAIL  <file> > <test>` in its failed-tests section.
pub(super) fn vitest(text: &str) -> Option<Vec<String>> {
    distinct(&VITEST_FAILED, text)
}

/// jest: `  ● <suite> › <test>` heading each failure, or `Test suite failed
/// to run`; `● Console` heads captured logs, not a failure.
pub(super) fn jest(text: &str) -> Option<Vec<String>> {
    let failed: Vec<String> = distinct(&JEST_FAILED, text)?
        .into_iter()
        .filter(|name| name != "Console")
        .collect();
    (!failed.is_empty()).then_some(failed)
}

/// The distinct first captures of `re` in `text`, in first-seen order;
/// `None` when there is none.
fn distinct(re: &Regex, text: &str) -> Option<Vec<String>> {
    let mut found: Vec<String> = Vec::new();
    for name in names(re, text) {
        if !found.iter().any(|seen| seen == name) {
            found.push(name.to_string());
        }
    }
    (!found.is_empty()).then_some(found)
}

#[cfg(test)]
#[path = "failed_tests.rs"]
mod tests;
