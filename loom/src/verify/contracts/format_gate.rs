//! The static gate of `loom stage contracts freeze`: a frozen file cannot
//! change, so a contract or harness file that fails the stage's own formatter,
//! lint or search checks must be fixed before the freeze, not disputed after it.
//!
//! [`is_format_check`] picks the stage's acceptance commands that only check
//! formatting; [`format_problems`] runs those through the acceptance runner,
//! so setup, `${..}` expansion, timeouts and confinement match
//! `loom stage complete`. [`is_pure_check`] widens that to lints and searches
//! that read files without rewriting them; `static_problems` runs the ones
//! [`runs_at_freeze`] admits and blames only failures that name a file the
//! contract session wrote ([`attribute_failure`]).

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};

use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::{
    AcceptanceCriterion, CommandConfinement, Stage, StageSandboxConfig, TruthCheck,
};
use crate::testrun::recognize::{command_args, has_flag, invocations};
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::criteria::{
    run_acceptance_with_config, CriteriaConfig, CriterionResult, DEFAULT_CHECK_TIMEOUT,
};

/// Formatters that check without rewriting when one of the flags is present:
/// the words that start the command, then the flags (any one suffices).
const FLAG_CHECKS: [(&[&str], &[&str]); 6] = [
    (&["cargo", "fmt"], &["--check"]),
    (&["rustfmt"], &["--check"]),
    (&["prettier"], &["--check", "-c"]),
    (&["oxfmt"], &["--check"]),
    (&["ruff", "format"], &["--check"]),
    (&["black"], &["--check"]),
];

/// Biome subcommands that only report unless one of [`BIOME_WRITE_FLAGS`] is set.
const BIOME_SUBCOMMANDS: [&[&str]; 2] = [&["biome", "format"], &["biome", "check"]];
const BIOME_WRITE_FLAGS: [&str; 3] = ["--write", "--fix", "--apply"];

/// A `format:check` package script, as the runner prefixes leave each form:
/// `yarn [run] format:check` reads `["format:check"]`.
const SCRIPT_CHECKS: [&[&str]; 5] = [
    &["format:check"],
    &["npm", "run", "format:check"],
    &["bun", "run", "format:check"],
    &["pnpm", "run", "format:check"],
    &["pnpm", "format:check"],
];

/// Whether `command`, run from `cwd`, is a formatter check: a simple command
/// that checks formatting without rewriting files. A compound command is one
/// only when every simple command in it is: the freeze runs while the
/// contracts are red, so the other half of `cargo fmt --check && cargo test`
/// would fail every freeze.
pub fn is_format_check(command: &str, cwd: &Path) -> bool {
    let argvs = invocations(command, cwd);
    !argvs.is_empty() && argvs.iter().all(|argv| is_format_check_argv(argv))
}

fn is_format_check_argv(argv: &[String]) -> bool {
    let flagged = FLAG_CHECKS.iter().any(|(words, flags)| {
        command_args(argv, words).is_some_and(|args| flags.iter().any(|f| has_flag(&args, f)))
    });
    flagged || is_biome_check(argv) || is_script_check(argv)
}

fn is_biome_check(argv: &[String]) -> bool {
    BIOME_SUBCOMMANDS.iter().any(|words| {
        command_args(argv, words)
            .is_some_and(|args| !BIOME_WRITE_FLAGS.iter().any(|f| has_flag(&args, f)))
    })
}

fn is_script_check(argv: &[String]) -> bool {
    SCRIPT_CHECKS
        .iter()
        .any(|words| command_args(argv, words).is_some())
}

/// The most of a failing check's output a static problem quotes.
const OUTPUT_QUOTE_BYTES: usize = 2048;

/// What a pure check reads, for the freeze's admission rule.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Format,
    /// A lint that parses files without resolving the crate or package graph.
    Syntactic,
    /// A lint backed by a compiler: it fails on a red contract that does not build.
    Compiled,
    /// `tsc --noEmit`: the vitest adapter reports no build failure, so a red
    /// TypeScript contract naming an unwritten member still "compiles" and
    /// the check would blame the contract file.
    Tsc,
    Search,
}

/// The kind of a simple command, or `None` when it may write files or run
/// anything else. Autofix forms are never pure: they rewrite files the
/// freeze is about to hash.
fn kind_of(argv: &[String]) -> Option<Kind> {
    if is_format_check_argv(argv) {
        return Some(Kind::Format);
    }
    let argv = argv.strip_prefix(&["!".to_string()]).unwrap_or(argv);
    let reads = |words: &[&str], writes: &[&str]| {
        command_args(argv, words).is_some_and(|args| !writes.iter().any(|f| has_flag(&args, f)))
    };
    if reads(&["rg"], &[]) || reads(&["grep"], &[]) {
        Some(Kind::Search)
    } else if reads(&["eslint"], &["--fix"])
        || reads(&["biome", "lint"], &BIOME_WRITE_FLAGS)
        || reads(&["ruff", "check"], &["--fix", "--unsafe-fixes"])
    {
        Some(Kind::Syntactic)
    } else if reads(&["cargo", "clippy"], &["--fix"])
        || reads(&["go", "vet"], &[])
        || reads(&["golangci-lint", "run"], &["--fix"])
    {
        Some(Kind::Compiled)
    } else if command_args(argv, &["tsc"]).is_some_and(|args| has_flag(&args, "--noEmit")) {
        Some(Kind::Tsc)
    } else {
        None
    }
}

/// The kinds of every simple command in `command`; `None` when one is not a
/// pure check or there is none (a compound `cargo clippy && cargo test` is
/// not pure, as for formatters).
fn kinds(command: &str, cwd: &Path) -> Option<Vec<Kind>> {
    let argvs = invocations(command, cwd);
    let kinds: Option<Vec<Kind>> = argvs.iter().map(|argv| kind_of(argv)).collect();
    kinds.filter(|kinds| !kinds.is_empty())
}

/// Whether `command`, run from `cwd`, only reads files: a formatter check, a
/// lint without its autofix flag, or a search.
pub fn is_pure_check(command: &str, cwd: &Path) -> bool {
    kinds(command, cwd).is_some()
}

/// Whether the freeze runs `command`: a pure check that is not `tsc`, and a
/// compiler-backed one only when `contracts_compile`. A red contract that
/// does not build makes a compiler-backed lint report the missing API in the
/// contract file, which is the expected red state, not a defect.
pub fn runs_at_freeze(command: &str, cwd: &Path, contracts_compile: bool) -> bool {
    kinds(command, cwd).is_some_and(|kinds| {
        kinds.iter().all(|kind| match kind {
            Kind::Compiled => contracts_compile,
            Kind::Tsc => false,
            Kind::Format | Kind::Syntactic | Kind::Search => true,
        })
    })
}

/// The entries of `written` that `output` names as a whole path, sorted and
/// deduplicated. An entry matches where nothing in `[A-Za-z0-9_./-]` precedes
/// it (a leading `./` is skipped) and nothing in `[A-Za-z0-9_-]` follows it,
/// so `tests/foo.rs` is not blamed for `my_tests/foo.rs`.
pub fn attribute_failure(output: &str, written: &[String]) -> Vec<String> {
    let named: BTreeSet<&String> = written
        .iter()
        .filter(|path| !path.is_empty() && names_path(output, path))
        .collect();
    named.into_iter().cloned().collect()
}

fn names_path(output: &str, path: &str) -> bool {
    let bytes = output.as_bytes();
    let step = path.chars().next().map_or(1, char::len_utf8);
    let mut from = 0;
    while let Some(found) = output[from..].find(path) {
        let start = from + found;
        let followed = bytes
            .get(start + path.len())
            .is_some_and(|&b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'));
        if !followed && starts_a_path(bytes, start) {
            return true;
        }
        from = start + step;
    }
    false
}

fn is_path_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'/' | b'-')
}

/// Whether a path begins at `start`: nothing path-like precedes it, or only
/// a `./` that itself begins the word.
fn starts_a_path(bytes: &[u8], start: usize) -> bool {
    match start.checked_sub(1).map(|i| bytes[i]) {
        None => true,
        Some(prev) if !is_path_byte(prev) => true,
        Some(b'/') => {
            start >= 2
                && bytes[start - 2] == b'.'
                && (start == 2 || !is_path_byte(bytes[start - 3]))
        }
        Some(_) => false,
    }
}

/// The stage's acceptance and `after_stage` checks the freeze runs, each with
/// the label its problem carries. Acceptance formatter checks belong to
/// [`format_problems`]; an `after_stage` check keeps its 30 s default, not the
/// acceptance runner's longer one.
fn selected_checks(
    stage: &Stage,
    cwd: &Path,
    contracts_compile: bool,
) -> Vec<(&'static str, AcceptanceCriterion)> {
    let acceptance = stage
        .acceptance
        .iter()
        .filter(|criterion| !is_format_check(criterion.command(), cwd))
        .map(|criterion| ("acceptance criterion", criterion.clone()));
    let after_stage = stage.after_stage.iter().map(|check| {
        let timeout_secs = Some(check.timeout(DEFAULT_CHECK_TIMEOUT).as_secs());
        let check = TruthCheck {
            timeout_secs,
            ..check.clone()
        };
        (
            "after_stage criterion",
            AcceptanceCriterion::Extended(check),
        )
    });
    acceptance
        .chain(after_stage)
        .filter(|(_, criterion)| runs_at_freeze(criterion.command(), cwd, contracts_compile))
        .collect()
}

/// The stage's lint, search and `after_stage` formatter checks that fail on a
/// file in `written`, run as [`format_problems`] runs its checks; one problem
/// per such check. A failure that names no written file is left to
/// the stage's own completion, and a timed-out check gives no problem.
pub(crate) fn static_problems(
    stage: &Stage,
    worktree_root: &Path,
    working_dir: &Path,
    confinement: CommandConfinement,
    written: &[String],
    contracts_compile: bool,
) -> Result<Vec<String>> {
    let selected = selected_checks(stage, working_dir, contracts_compile);
    if selected.is_empty() {
        return Ok(Vec::new());
    }
    let run = Stage {
        acceptance: selected.iter().map(|(_, c)| c.clone()).collect(),
        setup: stage.setup.clone(),
        sandbox: StageSandboxConfig {
            command_confinement: Some(confinement),
            ..StageSandboxConfig::default()
        },
        ..Stage::default()
    };
    let config = CriteriaConfig::default().with_tree_root(Some(worktree_root));
    let outcome = run_acceptance_with_config(&run, Some(working_dir), &config)
        .context("Failed to run the stage's lint and search checks")?;
    Ok(outcome
        .results()
        .iter()
        .filter(|result| !result.passed() && !result.timed_out)
        .filter_map(|result| {
            let label = selected
                .iter()
                .find(|(_, criterion)| criterion.command() == result.command)
                .map_or("acceptance criterion", |(label, _)| *label);
            blame(label, result, written)
        })
        .collect())
}

/// The problem a failed check gives when its output names a written file:
/// loom's own line, then the output as one quoted run-output line (it is
/// attacker-influenced text).
fn blame(label: &str, result: &CriterionResult, written: &[String]) -> Option<String> {
    let output = format!("{}\n{}", result.stdout, result.stderr);
    let files = attribute_failure(&output, written);
    if files.is_empty() {
        return None;
    }
    let quoted = run_output_line(&truncate_to_byte_limit(&output, OUTPUT_QUOTE_BYTES));
    Some(format!(
        "{label} `{}` fails on a file the contract session wrote ({}):\n{quoted}",
        result.command,
        files.join(", ")
    ))
}

/// The stage's acceptance commands that are formatter checks, run as
/// acceptance runs them (from `working_dir`, inside `worktree_root`); one
/// problem per check that fails, naming its command.
pub fn format_problems(
    acceptance: &[AcceptanceCriterion],
    setup: &[String],
    worktree_root: &Path,
    working_dir: &Path,
    confinement: CommandConfinement,
) -> Result<Vec<String>> {
    let checks: Vec<AcceptanceCriterion> = acceptance
        .iter()
        .filter(|criterion| is_format_check(criterion.command(), working_dir))
        .cloned()
        .collect();
    if checks.is_empty() {
        return Ok(Vec::new());
    }
    let stage = Stage {
        acceptance: checks,
        setup: setup.to_vec(),
        sandbox: StageSandboxConfig {
            command_confinement: Some(confinement),
            ..StageSandboxConfig::default()
        },
        ..Stage::default()
    };
    let config = CriteriaConfig::default().with_tree_root(Some(worktree_root));
    let outcome = run_acceptance_with_config(&stage, Some(working_dir), &config)
        .context("Failed to run the stage's formatter checks")?;
    Ok(outcome
        .results()
        .iter()
        .filter(|result| !result.passed())
        .map(|result| {
            format!(
                "acceptance criterion `{}` fails on the contract files: run the repository's \
                 formatter over them",
                result.command
            )
        })
        .collect())
}

#[cfg(test)]
#[path = "format_gate_tests.rs"]
mod tests;
