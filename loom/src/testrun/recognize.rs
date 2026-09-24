//! Helpers every adapter shares.
//!
//! - Invocations: [`invocations`] splits a shell command into the argv of each
//!   simple command, resolving package-script indirection (`npm test` runs
//!   `package.json`'s `scripts.test`).
//! - Prefixes: [`command_args`] and [`strip_prefixes`] see through the wrappers
//!   a runner is started with (`env`, `bunx`, `uv run`, `cargo +nightly`, ...).
//! - Flags: [`has_flag`], [`positionals`], [`split_double_dash`].
//! - Output: [`pattern`], [`combined_output`], [`strip_ansi`], [`sum_matches`],
//!   [`last_match`], [`count_matches`], [`summary_from_counts`].

use std::borrow::Cow;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::{RunOutput, RunSummary};
use crate::plan::schema::validation::shell_lex::{lex, simple_commands, Word};

/// How many nested `scripts.test` levels [`invocations`] follows, so a script
/// that runs itself (`"test": "npm test"`) terminates.
const SCRIPT_DEPTH: usize = 3;

/// Package-manager commands that run `package.json`'s `test` script.
const SCRIPT_RUNNERS: [&[&str]; 7] = [
    &["npm", "test"],
    &["npm", "run", "test"],
    &["bun", "run", "test"],
    &["pnpm", "test"],
    &["pnpm", "run", "test"],
    &["yarn", "test"],
    &["yarn", "run", "test"],
];

/// `env` options that take a value word.
const ENV_VALUE_FLAGS: &[&str] = &["-u", "--unset", "-C", "--chdir"];
/// `bunx`/`npx` options that take a value word.
const NPX_VALUE_FLAGS: &[&str] = &["-p", "--package"];
/// `uv run` options that take a value word.
const UV_RUN_VALUE_FLAGS: &[&str] = &[
    "--with",
    "--with-editable",
    "--with-requirements",
    "-p",
    "--python",
    "--project",
    "--directory",
    "--package",
    "--extra",
    "--group",
    "--env-file",
    "--index",
    "--index-url",
    "--extra-index-url",
];

static ANSI: LazyLock<Regex> =
    LazyLock::new(|| pattern(r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)|[@-_])"));

/// The argv of every simple command in `command` (lexed with `shell_lex`), in
/// order, with word values (quotes removed). A package-script invocation
/// (`npm test`, `npm run test`, `bun run test`, `pnpm test`, `yarn test`,
/// optionally after `env`/`VAR=x`) is replaced by the simple commands of the
/// `scripts.test` in the `package.json` of its cwd, its extra arguments
/// appended to the script's last command; it stays as written when there is no
/// such script. `cd <dir>` moves that cwd for the commands after it and is
/// left out.
pub fn invocations(command: &str, cwd: &Path) -> Vec<Vec<String>> {
    let mut argvs = Vec::new();
    expand(command, cwd, SCRIPT_DEPTH, &mut argvs);
    argvs
}

fn expand(command: &str, cwd: &Path, depth: usize, argvs: &mut Vec<Vec<String>>) {
    let tokens = lex(command);
    let mut dir = cwd.to_path_buf();
    for simple in simple_commands(&tokens) {
        let argv: Vec<String> = simple.words.iter().map(|w| w.value.clone()).collect();
        if let Some(target) = cd_target(&argv) {
            dir = dir.join(target);
            continue;
        }
        let script = if depth == 0 {
            None
        } else {
            package_script(&argv, &dir)
        };
        match script {
            Some((script, extra)) => {
                let first = argvs.len();
                expand(&script, &dir, depth - 1, argvs);
                if let Some(last) = argvs[first..].last_mut() {
                    last.extend(extra);
                }
            }
            None => argvs.push(argv),
        }
    }
}

/// The directory of a `cd <dir>` command.
fn cd_target(argv: &[String]) -> Option<&String> {
    match argv {
        [cd, target] if program_name(cd) == "cd" => Some(target),
        _ => None,
    }
}

/// The `scripts.test` command of `dir/package.json` and the arguments passed
/// to it, when `argv` runs that script.
fn package_script(argv: &[String], dir: &Path) -> Option<(String, Vec<String>)> {
    let argv = strip_environment(argv);
    let rest = SCRIPT_RUNNERS
        .iter()
        .find_map(|words| after_words(argv, words))?;
    let manifest = fs::read_to_string(dir.join("package.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest).ok()?;
    let script = manifest.get("scripts")?.get("test")?.as_str()?.to_string();
    let extra = match rest.split_first() {
        Some((first, tail)) if first == "--" => tail,
        _ => rest,
    };
    Some((script, extra.to_vec()))
}

/// The arguments after `words` when `argv`, runner prefixes stripped, runs
/// `words`: `command_args(argv, &["cargo", "test"])`. The first word compares
/// by program name, so `/usr/bin/ctest` matches `ctest` and `./gradlew`
/// matches `gradlew`; the rest compare exactly. `None` when `argv` runs
/// something else.
pub fn command_args(argv: &[String], words: &[&str]) -> Option<Vec<String>> {
    let command = strip_prefixes(argv);
    after_words(&command, words).map(<[String]>::to_vec)
}

/// `argv` without the runner prefixes in front of the command it runs, as often
/// as they nest: `VAR=x` assignments, `env [options] [VAR=x]...`,
/// `bunx`/`npx [options]`, `pnpm exec`/`pnpm dlx`, `yarn [run|exec]`,
/// `python`/`python3 -m`, `uv run [options]`, `bundle exec`, and the
/// `+<toolchain>` of `cargo +<toolchain>`. `uv run python -m pytest -q`
/// becomes `pytest -q`. `cd <dir> &&` is a separate simple command already.
pub fn strip_prefixes(argv: &[String]) -> Vec<String> {
    let mut rest = strip_environment(argv);
    loop {
        let next = strip_environment(strip_wrapper(rest));
        if next.len() == rest.len() {
            break;
        }
        rest = next;
    }
    let mut command = rest.to_vec();
    let is_cargo = command.first().is_some_and(|w| program_name(w) == "cargo");
    if is_cargo && command.get(1).is_some_and(|w| w.starts_with('+')) {
        command.remove(1);
    }
    command
}

/// `argv` without leading `VAR=x` assignments and `env [options]` wrappers.
fn strip_environment(mut argv: &[String]) -> &[String] {
    loop {
        match argv.split_first() {
            Some((first, rest)) if is_assignment(first) => argv = rest,
            Some((first, rest)) if program_name(first) == "env" => {
                argv = skip_options(rest, ENV_VALUE_FLAGS);
            }
            _ => return argv,
        }
    }
}

/// `argv` without one runner wrapper at its front, or unchanged.
fn strip_wrapper(argv: &[String]) -> &[String] {
    let first = argv.first().map(|word| program_name(word));
    match (first, argv.get(1).map(String::as_str)) {
        (Some("bunx" | "npx"), _) => skip_options(&argv[1..], NPX_VALUE_FLAGS),
        (Some("pnpm"), Some("exec" | "dlx"))
        | (Some("bundle"), Some("exec"))
        | (Some("yarn"), Some("run" | "exec")) => &argv[2..],
        (Some("yarn"), _) => &argv[1..],
        (Some("uv"), Some("run")) => skip_options(&argv[2..], UV_RUN_VALUE_FLAGS),
        (Some(name), Some("-m")) if is_python(name) => &argv[2..],
        _ => argv,
    }
}

/// `args` without its leading options; a bare `--` ends them and is dropped.
fn skip_options<'a>(args: &'a [String], value_flags: &[&str]) -> &'a [String] {
    let mut index = 0;
    while let Some(word) = args.get(index) {
        if word == "--" {
            return &args[index + 1..];
        }
        if !is_flag(word) {
            break;
        }
        let takes_value = value_flags.contains(&word.as_str());
        index += if takes_value { 2 } else { 1 };
    }
    args.get(index..).unwrap_or_default()
}

/// The rest of `argv` after `words`, the first compared by program name.
fn after_words<'a>(argv: &'a [String], words: &[&str]) -> Option<&'a [String]> {
    let (program, subcommand) = words.split_first()?;
    if argv.len() < words.len() {
        return None;
    }
    let (head, tail) = argv.split_at(words.len());
    let matches = program_name(&head[0]) == *program && head[1..] == *subcommand;
    matches.then_some(tail)
}

/// The program a command word names: the word without its directory.
fn program_name(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

fn is_python(name: &str) -> bool {
    name.strip_prefix("python")
        .is_some_and(|version| version.chars().all(|c| c.is_ascii_digit() || c == '.'))
}

fn is_assignment(word: &str) -> bool {
    let word = Word {
        raw: word.to_string(),
        ..Word::default()
    };
    word.assignment_name().is_some()
}

/// Whether `word` is an option: it starts with `-` and is neither `-` nor `--`.
fn is_flag(word: &str) -> bool {
    word.starts_with('-') && word != "-" && word != "--"
}

/// Whether `args` holds `flag`, bare or as `flag=value`. Every word is checked,
/// including those after a bare `--`; split with [`split_double_dash`] first
/// when the side matters.
pub fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|word| {
        word.strip_prefix(flag)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
    })
}

/// The words of `args` that are neither options nor option values.
/// `value_flags` lists the options that take a separate value word
/// (`--manifest-path`, `-p`); `--flag=value` needs no listing. Every word after
/// a bare `--` is positional.
pub fn positionals<'a>(args: &'a [String], value_flags: &[&str]) -> Vec<&'a str> {
    let (options, operands) = split_double_dash(args);
    let mut found = Vec::new();
    let mut words = options.iter();
    while let Some(word) = words.next() {
        if !is_flag(word) {
            found.push(word.as_str());
        } else if value_flags.contains(&word.as_str()) {
            words.next();
        }
    }
    found.extend(operands.iter().map(String::as_str));
    found
}

/// `args` split at its first bare `--`: the words before it and the words after
/// it (empty when there is no `--`).
pub fn split_double_dash(args: &[String]) -> (&[String], &[String]) {
    match args.iter().position(|word| word == "--") {
        Some(index) => (&args[..index], &args[index + 1..]),
        None => (args, &[]),
    }
}

/// `source` compiled. Adapters build their fixed output patterns with it in
/// `LazyLock<Regex>` statics; an invalid pattern is a bug and panics naming it.
pub fn pattern(source: &str) -> Regex {
    match Regex::new(source) {
        Ok(regex) => regex,
        Err(error) => panic!("invalid output pattern {source:?}: {error}"),
    }
}

/// `text` without ANSI escape sequences (colour, cursor and OSC codes).
pub fn strip_ansi(text: &str) -> Cow<'_, str> {
    ANSI.replace_all(text, "")
}

/// A run's stdout, a newline, then its stderr, ANSI codes stripped from both.
pub fn combined_output(out: &RunOutput<'_>) -> String {
    format!("{}\n{}", strip_ansi(out.stdout), strip_ansi(out.stderr))
}

/// The sum of capture group 1, read as a number, over every match of `re` in
/// `text`; `None` when nothing matches. Use it for counts a runner prints once
/// per test binary or file (`running (\d+) tests?`).
pub fn sum_matches(re: &Regex, text: &str) -> Option<u64> {
    captured_numbers(re, text).reduce(u64::saturating_add)
}

/// Capture group 1, read as a number, of the last match of `re` in `text`. Use
/// it for a summary line printed once at the end of a run.
pub fn last_match(re: &Regex, text: &str) -> Option<u64> {
    captured_numbers(re, text).last()
}

/// How many times `re` matches in `text`: one line per test (`--- FAIL: `).
pub fn count_matches(re: &Regex, text: &str) -> u64 {
    let matches = re.find_iter(text).count();
    u64::try_from(matches).unwrap_or(u64::MAX)
}

fn captured_numbers<'t>(re: &'t Regex, text: &'t str) -> impl Iterator<Item = u64> + 't {
    re.captures_iter(text)
        .filter_map(|captures| captures.get(1)?.as_str().parse().ok())
}

/// A summary from a runner's passed/failed/skipped counts: `executed` is
/// passed + failed, or `None` when neither was reported.
pub fn summary_from_counts(
    passed: Option<u64>,
    failed: Option<u64>,
    skipped: Option<u64>,
) -> RunSummary {
    let executed = match (passed, failed) {
        (None, None) => None,
        _ => Some(passed.unwrap_or(0).saturating_add(failed.unwrap_or(0))),
    };
    RunSummary {
        executed,
        passed,
        failed,
        skipped,
        build_failed: false,
    }
}
