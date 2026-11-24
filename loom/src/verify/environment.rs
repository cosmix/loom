//! Output text that means the environment, not the code under test, failed:
//! a refused sandbox, a missing network or a full disk. The one owner of the
//! markers: the contract freeze, the dry run, acceptance rendering and the
//! impact judge all read them here.

use std::path::Path;

use crate::testrun::{registry, RunOutput};

/// Lowercase output fragments only a refused launcher or sandbox prints. A
/// test's own assertion text does not normally hold them.
const LAUNCHER_MARKERS: [&str; 2] = ["bwrap:", "operation not permitted"];

/// Lowercase output fragments of a refused sandbox that a test's own text can
/// hold: a test of signals names `EPERM`. Only [`sandbox_denial`] reads them,
/// and [`run_denial`] only for a run with no verdict of its own. A fragment
/// that starts or ends in a word character matches only as a whole token, so
/// `eperm` names `Error: kill EPERM` and not a longer identifier.
///
/// Residual: an unparsed failing run whose own output prints `EPERM` or
/// "operation not permitted" becomes an impact note, not a failure. The full
/// suite in integration-verify is the backstop.
const DENIAL_MARKERS: [&str; 4] = [
    "eperm",
    "sandbox_apply",
    "deny(1)",
    "couldntreadcurrentdirectory",
];

/// Lowercase output fragments of a missing network or a full disk.
const HOST_MARKERS: [&str; 7] = [
    "temporary failure in name resolution",
    "could not resolve host",
    "couldn't resolve host",
    "network is unreachable",
    "connection refused",
    "read-only file system",
    "no space left on device",
];

/// Lowercase socket error codes a refused loopback bind or dial prints. Codes
/// only: "failed to start server" also opens unrelated failures.
const BIND_REFUSAL_MARKERS: [&str; 2] = ["eaddrinuse", "failedtoopensocket"];

/// The most bytes of a denial line a message carries.
const DENIAL_LINE_MAX_BYTES: usize = 300;

/// The marker found in `stdout` or `stderr`, when the run failed for an
/// environment reason: a refused sandbox, a missing network or a full disk.
/// Only markers a test's own failure text does not normally hold, as the
/// freeze and the contract check apply it even to a run whose summary parsed.
pub fn environment_marker(stdout: &str, stderr: &str) -> Option<&'static str> {
    let output = format!("{stderr}\n{stdout}").to_lowercase();
    LAUNCHER_MARKERS
        .into_iter()
        .chain(HOST_MARKERS)
        .find(|marker| has_marker(&output, marker))
}

/// The first line of `output` that shows the sandbox refused the command,
/// trimmed and cut to at most 300 bytes on a character boundary; `None` for
/// output with no such line. The match ignores case.
pub fn sandbox_denial(output: &str) -> Option<String> {
    let line = output.lines().find(|line| {
        let lower = line.to_lowercase();
        LAUNCHER_MARKERS
            .iter()
            .chain(&DENIAL_MARKERS)
            .any(|marker| has_marker(&lower, marker))
    })?;
    Some(cut_line(line))
}

/// `line` trimmed and cut to [`DENIAL_LINE_MAX_BYTES`] on a character boundary.
fn cut_line(line: &str) -> String {
    let line = line.trim();
    let end = line.floor_char_boundary(DENIAL_LINE_MAX_BYTES);
    line[..end].to_string()
}

/// The note for a run that most likely failed because macOS confinement gave it
/// no loopback networking: [`bind_refusal_hint`] when `loopback_refused` and the
/// run neither timed out nor exited zero. It does not consult [`run_denial`]: a
/// run that reached a test count still gets the note.
pub fn refused_bind_note(
    loopback_refused: bool,
    timed_out: bool,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Option<String> {
    (loopback_refused && !timed_out && exit_code != Some(0))
        .then(|| bind_refusal_hint(stdout, stderr))
        .flatten()
}

/// The note naming the loopback opt-in when a line of `stderr`, then `stdout`,
/// holds a socket error code as a token; `None` when no line does. The match
/// ignores case and quotes the line as printed. No platform or exit-code gate:
/// see [`refused_bind_note`].
pub fn bind_refusal_hint(stdout: &str, stderr: &str) -> Option<String> {
    let line = stderr.lines().chain(stdout.lines()).find(|line| {
        let lower = line.to_lowercase();
        BIND_REFUSAL_MARKERS
            .iter()
            .any(|marker| has_marker(&lower, marker))
    })?;
    Some(format!(
        "refused bind? this run had no loopback networking (macOS confinement) and printed: \
         {}; tests that listen or dial on loopback need \
         sandbox.network.allow_local_binding: true in the plan or stage, which an operator \
         sets; the run still fails, and if it fails only because of where completion runs, \
         dispute it",
        cut_line(line)
    ))
}

/// The sandbox denial in a run's `stderr` then `stdout`, if any.
pub fn run_output_denial(stdout: &str, stderr: &str) -> Option<String> {
    sandbox_denial(&format!("{stderr}\n{stdout}"))
}

/// [`run_output_denial`] for a failed run of `command` that is no test run's
/// own verdict: output that reached a test count, or the compiler rejecting
/// the code, holds assertion and compiler text, and `EPERM` there names what a
/// test checked, not what the sandbox refused. The caller has ruled out a
/// timeout and a zero exit. `command` names the adapter that reads a build
/// failure; the counts are read by every adapter, as a wrapper script may run
/// any runner.
pub fn run_denial(
    command: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> Option<String> {
    let denial = run_output_denial(stdout, stderr)?;
    let output = RunOutput {
        stdout,
        stderr,
        exit_code,
    };
    let counted = registry::all().iter().any(|adapter| {
        let summary = adapter.parse(&output);
        summary.executed.is_some() || summary.failed.is_some_and(|failed| failed > 0)
    });
    let rejected = registry::recognize(command, Path::new("."))
        .is_some_and(|adapter| adapter.compile_rejected(&output));
    (!counted && !rejected).then_some(denial)
}

/// Whether the lowercase `text` holds `marker` as a token: a marker edge that
/// is a word character does not touch another word character.
fn has_marker(text: &str, marker: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(marker).any(|(start, found)| {
        let before = text[..start].chars().next_back();
        let after = text[start + found.len()..].chars().next();
        let opens = marker.chars().next().is_some_and(word);
        let closes = marker.chars().next_back().is_some_and(word);
        !(opens && before.is_some_and(word)) && !(closes && after.is_some_and(word))
    })
}
