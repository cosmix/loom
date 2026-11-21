//! Guard the test suite against a stand-in process that can hang it.
//!
//! A host job's sandbox can refuse a signal, and a process can trap `SIGTERM`.
//! A test that ignores the result of a kill and then waits for the process
//! without a bound blocks for as long as the process runs: a `Drop` that called
//! `killpg` and then `wait()` hung the lib test binary that way. The lib's
//! `test_stand_in` module skips such tests where signals are refused, checks
//! every kill and bounds every wait. The guard reads every place test code lives
//! (see `regions`) and rejects:
//!
//! 1. in `src/`, a kill sent outside `test_stand_in`: such a test owns a
//!    `StandIn`, an `Orphan` or an `AdoptedGroup` and drops it. Under `tests/`
//!    the lib's helper is out of reach and `sandbox_probe::reap` bounds the
//!    wait, so rules 2 and 3 apply there;
//! 2. anywhere, an unbounded wait on a stand-in (`.child().wait()`): its
//!    `wait_within` takes a bound;
//! 3. anywhere, even in a file allowed below, a kill whose result is discarded
//!    followed, in the same function, by an unbounded wait;
//! 4. in `src/`, `.wait()` or `.wait_with_output()` on a child that a `let`
//!    in the same function bound from `Command::...spawn()`, or from any
//!    statement ending in a no-argument `.spawn()`: a raw child the
//!    test waits on, for the code under test to end it, blocks for as long as
//!    a regressed kill leaves it running. A `StandIn`'s `wait_within` bounds
//!    it. A barrier or a condvar waits the same way, so a child reached any
//!    other way (a helper's return value, a field, a parameter) is not read.
//!
//! A kill is `kill` or `killpg` (nix or libc) with any signal but a stimulus,
//! `process::terminate` (a one-argument `terminate`), `Child::kill`, the
//! `kill` command, or `sandbox_probe::reap::kill_and_reap`, whose `true` says
//! the child ended: in `src/` a test owns its child through the helper
//! instead, while under `tests/` it is the documented owner and only rule 3
//! reads it. A signal held in a variable counts, and so does `SIGTERM`,
//! since a process can trap it. A stimulus is a test's input, named literally:
//! `SIGSTOP`, `SIGCONT`, `SIGINT`, `SIGHUP` or the null signal (`None`, `0`).
//! Aliases and other spellings are outside this textual guard.

mod allowances;
mod regions;

use std::path::Path;

use allowances::{ALLOWED, ALLOWED_CALLS};
use regions::{matching, test_regions, Region};

/// The guard's own files: they hold the patterns it rejects, as test input.
const GUARD_FILES: &str = "tests/integration/stand_in_signal_guard";

/// Signals a test sends as input, by name; the null signal is one too.
const STIMULI: &[&str] = &["SIGSTOP", "SIGCONT", "SIGINT", "SIGHUP"];

/// Which rules apply to a region besides the ones that apply everywhere.
#[derive(Clone, Copy, Default)]
struct Scope<'a> {
    /// Rule 1: a kill outside the helper is a finding.
    raw_kills: bool,
    /// Rule 4: a wait on a raw child is a finding.
    raw_waits: bool,
    /// Calls exempt from rules 1 and 4 here, as findings print them.
    allowed: &'a [&'a str],
}

/// A kill in squeezed code: where it starts and ends, and what it is.
struct Kill {
    at: usize,
    end: usize,
    what: String,
}

/// Every call of the free function `name` in `code`, as (start, end, argument
/// text). A method call (`.name(`), a longer name (`x_name(`) and a definition
/// (`fn name(`) are not calls of it.
fn calls<'a>(code: &'a str, name: &str) -> Vec<(usize, usize, &'a str)> {
    let needle = format!("{name}(");
    code.match_indices(&needle)
        .filter(|(at, _)| {
            let before = code[..*at].chars().next_back();
            let defined = code[..*at].ends_with("fn");
            !defined && !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.')
        })
        .filter_map(|(at, _)| {
            let open = at + name.len();
            matching(code, open).map(|close| (at, close + 1, &code[open + 1..close]))
        })
        .collect()
}

/// The index of each `separator` in `text` outside brackets and strings.
fn top_level(text: &str, separator: u8) -> Vec<usize> {
    let (mut depth, mut quoted, mut escaped) = (0i32, false, false);
    let mut found = Vec::new();
    for (i, byte) in text.bytes().enumerate() {
        match byte {
            _ if escaped => escaped = false,
            b'\\' if quoted => escaped = true,
            b'"' => quoted = !quoted,
            _ if quoted => {}
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ if byte == separator && depth == 0 => found.push(i),
            _ => {}
        }
    }
    found
}

/// `args`, a call's argument text, split at its top-level commas.
fn arguments(args: &str) -> Vec<&str> {
    let mut start = 0;
    let mut found = Vec::new();
    for comma in top_level(args, b',') {
        found.push(&args[start..comma]);
        start = comma + 1;
    }
    found.push(&args[start..]);
    found
}

/// Whether `signal`, the signal argument of a kill, names a stimulus.
fn stimulus(signal: &str) -> bool {
    let signal = signal
        .strip_prefix("Some(")
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(signal);
    let name = signal.rsplit("::").next().unwrap_or(signal);
    matches!(name, "None" | "0") || STIMULI.contains(&name)
}

fn kills(code: &str) -> Vec<Kill> {
    let mut found = Vec::new();
    for name in ["killpg", "kill"] {
        for (at, end, args) in calls(code, name) {
            if matches!(arguments(args)[..], [_, signal] if !stimulus(signal)) {
                let what = format!("{name}({args})");
                found.push(Kill { at, end, what });
            }
        }
    }
    for name in ["terminate", "kill_and_reap"] {
        for (at, end, args) in calls(code, name) {
            if !args.is_empty() && arguments(args).len() == 1 {
                let what = format!("{name}({args})");
                found.push(Kill { at, end, what });
            }
        }
    }
    for needle in [".kill()", ".start_kill()", "Command::new(\"kill\")"] {
        for (at, _) in code.match_indices(needle) {
            let what = needle.trim_start_matches('.').to_string();
            found.push(Kill {
                at,
                end: at + needle.len(),
                what,
            });
        }
    }
    found
}

/// Whether the kill at `kill` discards its result.
fn unchecked(code: &str, kill: &Kill) -> bool {
    let after = &code[kill.end..];
    let before = &code[..kill.at];
    let checks = [
        ".unwrap()",
        ".expect(",
        "?",
        ".is_ok()",
        ".is_err()",
        "==",
        "!=",
        "}==",
    ];
    if checks.iter().any(|check| after.starts_with(check)) {
        return false;
    }
    let bound = before.rsplit(';').next().unwrap_or("");
    let named = bound.starts_with("let") && !bound.starts_with("let_");
    let tested = ["assert!(", "matches!(", "if!"];
    !(named || tested.iter().any(|test| before.ends_with(test)))
}

/// The body of the innermost `fn` around `at`, as a range of `code`.
fn enclosing_fn(code: &str, at: usize) -> std::ops::Range<usize> {
    let starts = code[..at]
        .match_indices("fn")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    for start in starts.into_iter().rev() {
        let boundary = code[..start].chars().next_back();
        if boundary.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let Some(open) = code[start..].find('{').map(|i| start + i) else {
            continue;
        };
        if let Some(close) = matching(code, open) {
            if open < at && at < close {
                return open..close;
            }
        }
    }
    0..code.len()
}

fn unbounded_waits(code: &str) -> Vec<usize> {
    let mut found: Vec<usize> = [".wait()", ".wait_with_output()"]
        .iter()
        .flat_map(|needle| code.match_indices(needle).map(|(i, _)| i))
        .collect();
    let waitpid = calls(code, "waitpid").into_iter();
    found.extend(
        waitpid
            .filter(|(_, _, args)| !args.contains("WNOHANG"))
            .map(|c| c.0),
    );
    found
}

/// The expression a method call at `dot` is made on, such as `child` in
/// `child.wait()` or `cmd.spawn().unwrap()` in `cmd.spawn().unwrap().wait()`.
fn receiver(code: &str, dot: usize) -> &str {
    let bytes = code.as_bytes();
    let (mut start, mut depth) = (dot, 0usize);
    while start > 0 {
        match bytes[start - 1] {
            b')' | b']' => depth += 1,
            b'(' | b'[' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            byte if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.' => {}
            _ => break,
        }
        start -= 1;
    }
    &code[start..dot]
}

/// Whether the squeezed `statement` ends in a `.spawn()` with no arguments,
/// unwrapped or not: `command.stdout(..).spawn().unwrap()` does, and so does
/// `cmd.spawn()?`; `job.spawn("x")` is a helper's call and does not.
fn ends_in_spawn(statement: &str) -> bool {
    let Some(at) = statement.rfind(".spawn()") else {
        return false;
    };
    let mut rest = &statement[at + ".spawn()".len()..];
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('?') {
            rest = after;
        } else if let Some(after) = rest.strip_prefix(".unwrap()") {
            rest = after;
        } else if let Some(open) = [".expect(", ".unwrap_or_else("]
            .iter()
            .find(|unwrap| rest.starts_with(**unwrap))
            .map(|unwrap| unwrap.len() - 1)
        {
            let Some(close) = matching(rest, open) else {
                return false;
            };
            rest = &rest[close + 1..];
        } else {
            return false;
        }
    }
    true
}

/// Whether `name`, waited on at `at`, holds a child that a `let` in the same
/// function bound from `Command::...spawn()`, or from any statement ending in
/// a `.spawn()` (see `ends_in_spawn`), such as a command built in a variable.
fn spawned_here(code: &str, at: usize, name: &str) -> bool {
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return false;
    }
    let body = &code[enclosing_fn(code, at).start..at];
    let bindings = ["let", "letmut"].into_iter().flat_map(|keyword| {
        let needle = format!("{keyword}{name}");
        let found: Vec<usize> = body.match_indices(&needle).map(|(i, _)| i).collect();
        found.into_iter().filter(move |i| {
            let opens = i
                .checked_sub(1)
                .is_none_or(|j| b";{}".contains(&body.as_bytes()[j]));
            let rest = &body[i + needle.len()..];
            opens && (rest.starts_with('=') || rest.starts_with(':'))
        })
    });
    // A `let` still open at `at` is the one the wait's value goes to, as in
    // `let out = out.wait_with_output()`, and holds no child yet.
    let statements = bindings.filter_map(|binding| {
        let statement = &body[binding..];
        let end = top_level(statement, b';').first().copied()?;
        Some((binding, &statement[..end]))
    });
    statements.max().is_some_and(|(_, statement)| {
        (statement.contains("Command::") && statement.contains(".spawn()"))
            || ends_in_spawn(statement)
    })
}

/// Rule 4: each wait in `code` on a child bound from `Command::...spawn()`
/// in the same function, as `child.wait()`.
fn raw_waits(code: &str) -> Vec<String> {
    let mut found = Vec::new();
    for needle in [".wait()", ".wait_with_output()"] {
        for (at, _) in code.match_indices(needle) {
            let name = receiver(code, at);
            if spawned_here(code, at, name) {
                found.push(format!("{name}{needle}"));
            }
        }
    }
    found
}

/// What in `code` can hang a test, one line per finding, under `scope`.
fn findings(code: &str, scope: Scope) -> Vec<String> {
    let mut found = Vec::new();
    let kills = kills(code);
    if scope.raw_kills {
        let raw = kills
            .iter()
            .filter(|kill| !scope.allowed.contains(&&*kill.what));
        found.extend(raw.map(|kill| format!("a kill outside the helper: {}", kill.what)));
    }
    for needle in [".child().wait()", ".child().wait_with_output()"] {
        if code.contains(needle) {
            found.push(format!("an unbounded wait on a stand-in: {needle}"));
        }
    }
    let waits = unbounded_waits(code);
    for kill in kills.iter().filter(|kill| unchecked(code, kill)) {
        let body = enclosing_fn(code, kill.at);
        if waits
            .iter()
            .any(|wait| *wait > kill.end && *wait < body.end)
        {
            found.push(format!(
                "an unchecked kill, then an unbounded wait: {}",
                kill.what
            ));
        }
    }
    if scope.raw_waits {
        let raw = raw_waits(code).into_iter();
        let raw = raw.filter(|wait| !scope.allowed.contains(&wait.as_str()));
        found.extend(raw.map(|wait| format!("an unbounded wait on a raw child: {wait}")));
    }
    found
}

fn offenders(regions: &[Region]) -> Vec<String> {
    regions
        .iter()
        .filter(|region| !region.label.starts_with(GUARD_FILES))
        .filter_map(|region| {
            let file = region.label.split(" (").next().unwrap_or("");
            let in_src = file.starts_with("src/");
            let allowed: Vec<&str> = ALLOWED_CALLS
                .iter()
                .filter(|(path, _, _)| *path == file)
                .map(|(_, call, _)| *call)
                .collect();
            let scope = Scope {
                raw_kills: in_src && !ALLOWED.iter().any(|(path, _)| *path == file),
                raw_waits: in_src,
                allowed: &allowed,
            };
            let found = findings(&region.code, scope);
            (!found.is_empty()).then(|| format!("{}: {}", region.label, found.join("; ")))
        })
        .collect()
}

#[test]
fn no_test_can_hang_on_a_stand_in() {
    let regions = test_regions(Path::new(env!("CARGO_MANIFEST_DIR")));
    let inline = regions
        .iter()
        .filter(|r| r.label.contains("#[cfg(test)]"))
        .count();
    let under_tests = regions
        .iter()
        .filter(|r| r.label.starts_with("tests/"))
        .count();
    assert!(
        inline > 100 && under_tests > 100 && regions.len() > 1000,
        "found only {} regions ({inline} inline, {under_tests} under tests/)",
        regions.len()
    );
    let offenders = offenders(&regions);
    assert!(
        offenders.is_empty(),
        "this test code can hang on a process it signals; own it through \
         crate::test_stand_in (StandIn, Orphan or AdoptedGroup) in src/, or \
         loom::process::sandbox_probe::reap under tests/, and bound every wait:\n{}",
        offenders.join("\n")
    );
}

mod tests;
