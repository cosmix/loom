//! What a stopped run's console output says about the tests still running,
//! for the runners that print a test's name when it starts. Each reader
//! returns `None` unless the output shows its own runner's start lines; the
//! killed runs each one was written from are under
//! `fixtures/<adapter>/killed/`. A runner that names a test only when it
//! finishes has no reader here.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::recognize::{pattern, strip_ansi};
use super::RunOutput;

static GO_PACKAGE_END: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^(?:ok|FAIL)\s+\S+\s+(?:[\d.]+s|\(cached\)|\[[^\]\n]*\])"));
static GO_RUN: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^=== RUN\s+(\S+)$"));
static GO_END: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^\s*--- (?:PASS|FAIL|SKIP): (\S+) \("));
static PYTEST_SESSION: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"(?m)^=+ test session starts =+$"));
static PYTEST_STARTED: LazyLock<Regex> = LazyLock::new(|| pattern(r"^(\S+::.+?) $"));
static UNITTEST_TEST: LazyLock<Regex> = LazyLock::new(|| pattern(r"^\w+ \([\w.]+\)$"));
static MINITEST_RUNNING: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^# Running:$"));
static MINITEST_STARTED: LazyLock<Regex> = LazyLock::new(|| pattern(r"^(\S.*#.+?) = $"));
static EXUNIT_RUN: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^Running ExUnit with seed"));
static EXUNIT_MODULE: LazyLock<Regex> = LazyLock::new(|| pattern(r"^(\S+) \[[^\]]+\]$"));
static EXUNIT_STARTED: LazyLock<Regex> = LazyLock::new(|| pattern(r"^\s+\* (.+?) \[L#\d+\]$"));
static CTEST_PROJECT: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^Test project "));
static CTEST_SUMMARY: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^\d+% tests passed"));
static CTEST_START: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^\s*Start\s+(\d+): (.+)$"));
static CTEST_END: LazyLock<Regex> = LazyLock::new(|| pattern(r"(?m)^\s*\d+/\d+ Test\s+#(\d+): "));

/// `reader` applied to stdout, else to stderr, ANSI codes stripped.
pub(super) fn read_either(
    out: &RunOutput<'_>,
    reader: fn(&str) -> Option<Vec<String>>,
) -> Option<Vec<String>> {
    reader(&strip_ansi(out.stdout)).or_else(|| reader(&strip_ansi(out.stderr)))
}

/// `go test -v`: `=== RUN   <name>` when a test or subtest starts and
/// `--- PASS|FAIL|SKIP: <name> (...)` when it ends. A package's output ends
/// with its `ok` or `FAIL` line and the next package's starts after it, so
/// only the lines after the last such line count.
pub(super) fn go(text: &str) -> Option<Vec<String>> {
    let start = GO_PACKAGE_END
        .find_iter(text)
        .last()
        .map_or(0, |end| end.end());
    let block = &text[start..];
    let ended: HashSet<&str> = names(&GO_END, block).collect();
    let mut started = names(&GO_RUN, block).peekable();
    started.peek()?;
    let mut running: Vec<String> = Vec::new();
    for name in started.filter(|name| !ended.contains(name)) {
        if !running.iter().any(|seen| seen == name) {
            running.push(name.to_string());
        }
    }
    Some(running)
}

/// `pytest -v`: a test's node id and a space when it starts, its outcome
/// and progress after them on the same line when it ends. A last line that
/// holds only a node id is the test that was running.
pub(super) fn pytest(text: &str) -> Option<Vec<String>> {
    PYTEST_SESSION.find(text)?;
    let id = capture(&PYTEST_STARTED, last_line(text))?;
    Some(vec![id.to_string()])
}

/// `python -m unittest -v`: `<name> (<id>) ... ` when a test starts, its
/// result after it when it ends. A test with a docstring has the docstring's
/// first line on a line of its own between the two.
pub(super) fn unittest(text: &str) -> Option<Vec<String>> {
    let description = text.strip_suffix(" ... ")?;
    let mut lines = description.rsplit('\n');
    let last = lines.next()?;
    let test = if UNITTEST_TEST.is_match(last) {
        last
    } else {
        lines.next().filter(|line| UNITTEST_TEST.is_match(line))?
    };
    Some(vec![test.to_string()])
}

/// `ruby -Itest <file> -v` (minitest): `<Class>#<test> = ` when a test
/// starts, its time and result after it when it ends.
pub(super) fn minitest(text: &str) -> Option<Vec<String>> {
    MINITEST_RUNNING.find(text)?;
    let name = capture(&MINITEST_STARTED, last_line(text))?;
    Some(vec![name.to_string()])
}

/// `mix test --trace`: `  * <test> [L#<line>]` when a test starts, rewritten
/// after a carriage return with its time when it ends, under a
/// `<Module> [<file>]` heading.
pub(super) fn exunit(text: &str) -> Option<Vec<String>> {
    EXUNIT_RUN.find(text)?;
    let current = last_line(text).rsplit('\r').next()?;
    let test = capture(&EXUNIT_STARTED, current)?;
    let module = text
        .lines()
        .rev()
        .find_map(|line| capture(&EXUNIT_MODULE, line));
    Some(vec![match module {
        Some(module) => format!("{test} ({module})"),
        None => test.to_string(),
    }])
}

/// `ctest`: `Start <n>: <name>` when a test starts and `<i>/<total> Test
/// #<n>: <name> ...` when it ends, in parallel runs (`-j`) too. Only the last
/// `Test project` run counts, and one that printed its summary has ended.
pub(super) fn ctest(text: &str) -> Option<Vec<String>> {
    let start = CTEST_PROJECT.find_iter(text).last()?.start();
    let block = &text[start..];
    if CTEST_SUMMARY.is_match(block) {
        return None;
    }
    let ended: HashSet<&str> = names(&CTEST_END, block).collect();
    let running = CTEST_START
        .captures_iter(block)
        .filter_map(|found| Some((found.get(1)?.as_str(), found.get(2)?.as_str())))
        .filter(|(number, _)| !ended.contains(number))
        .map(|(_, name)| name.trim_end().to_string())
        .collect();
    Some(running)
}

/// The text after the last newline: a line the runner had not ended yet.
fn last_line(text: &str) -> &str {
    text.rsplit('\n').next().unwrap_or(text)
}

fn capture<'t>(re: &Regex, text: &'t str) -> Option<&'t str> {
    Some(re.captures(text)?.get(1)?.as_str())
}

/// The first capture group of every match of `re` in `text`.
pub(super) fn names<'t>(re: &'t Regex, text: &'t str) -> impl Iterator<Item = &'t str> + 't {
    re.captures_iter(text)
        .filter_map(|captures| Some(captures.get(1)?.as_str()))
}
