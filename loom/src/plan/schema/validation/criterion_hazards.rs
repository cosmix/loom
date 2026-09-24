//! Lints for stage commands that cannot mean what they say inside a stage
//! sandbox: acceptance criteria, setup commands and wiring tests.
//!
//! Three hazards are errors because the command's result is meaningless: an
//! exit status masked with `|| true`, `HOME` pointed at a variable or
//! substitution, and a bare `mktemp -d`. The rest are warnings: they usually
//! fail in the sandbox or pass without testing anything.
//!
//! Commands are lexed (see `shell_lex`), so text inside a quoted argument is
//! data: an `rg` or `grep` pattern is never itself read as a hazard.

use std::collections::BTreeSet;

use super::super::types::{StageDefinition, ValidationError};
use super::search_args::SearchArgs;
use super::shell_lex::{lex, simple_commands, SimpleCommand, Word};

mod masked_exit;
use masked_exit::masks_exit_status;

/// How deep the scan follows command substitutions and `sh -c` scripts.
pub(super) const MAX_NESTING: usize = 4;

/// Words that put the word after them in command position.
const KEYWORDS: [&str; 11] = [
    "!", "if", "then", "else", "elif", "do", "while", "until", "{", "}", "time",
];

/// Commands that run the next word as the command.
const WRAPPERS: [&str; 5] = ["command", "exec", "nohup", "builtin", "env"];

/// Builtins whose non-option arguments are `NAME=value` declarations, not
/// argv passed to another program — `export HOME=$x` sets `$HOME`, unlike
/// `awk -v HOME=$x` where `HOME` names an awk variable.
const DECLARATION_BUILTINS: [&str; 5] = ["export", "declare", "typeset", "local", "readonly"];

/// Package runners that launch the tool named after them.
const LAUNCHERS: [&str; 15] = [
    "npx", "bunx", "bun", "pnpm", "yarn", "npm", "uv", "uvx", "poetry", "python", "python3", "run",
    "exec", "x", "-m",
];

/// A hazard found in a stage command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Hazard {
    MaskedExit,
    HomeFromExpansion,
    BareMktempDir,
    Network(&'static str),
    PlanPath,
    VitestNameFilter,
    PipeStatus,
    RgReplace,
    TestRunnerInWiring,
    HardcodedTmp,
}

impl Hazard {
    /// Errors make the command's result meaningless; the rest are warnings.
    pub(crate) fn is_error(self) -> bool {
        matches!(
            self,
            Self::MaskedExit | Self::HomeFromExpansion | Self::BareMktempDir
        )
    }

    pub(crate) fn message(self) -> String {
        let text = match self {
            Self::MaskedExit => {
                "masks its exit status (a final `|| true`, `|| :`, `|| exit 0`, `; true` or \
                 `; :`), so it passes whatever happens"
            }
            Self::HomeFromExpansion => {
                "assigns HOME from a variable or command substitution, so tools that keep \
                 toolchains, caches or config under HOME start empty"
            }
            Self::BareMktempDir => {
                "runs a bare `mktemp -d`; write `mktemp -d \"${TMPDIR:-/tmp}/<name>.XXXXXX\"` \
                 so the directory lands in the sandbox's writable TMPDIR"
            }
            Self::Network(tool) => {
                return format!(
                    "runs `{tool}`, which needs network access the stage sandbox does not grant"
                );
            }
            Self::PlanPath => {
                "reads a path under doc/plans/, which the plan lifecycle renames; check the \
                 code the stage changes instead"
            }
            Self::VitestNameFilter => {
                "filters vitest with -t; a name filter that matches no test still exits 0"
            }
            Self::PipeStatus => "reads PIPESTATUS, which only bash sets; criteria run under sh -c",
            Self::RgReplace => {
                "passes -r to rg, where it means --replace rather than recursive; rg already \
                 recurses"
            }
            Self::TestRunnerInWiring => {
                "runs a test runner inside wiring_tests; wiring tests exercise the built entry \
                 point, and test runs belong in acceptance"
            }
            Self::HardcodedTmp => {
                "hardcodes /tmp/; use \"${TMPDIR:-/tmp}/...\" or a TMPDIR= prefix assignment so \
                 it stays in the sandbox's writable directory"
            }
        };
        text.to_string()
    }
}

/// Push a `ValidationError` for every error-level hazard in the stage's commands.
pub(crate) fn push_hazard_errors(stage: &StageDefinition, errors: &mut Vec<ValidationError>) {
    for (label, command, in_wiring_test) in stage_commands(stage) {
        for hazard in scan(command, in_wiring_test) {
            if hazard.is_error() {
                errors.push(ValidationError {
                    message: format!("{label} `{command}` {}", hazard.message()),
                    stage_id: Some(stage.id.clone()),
                });
            }
        }
    }
}

/// Push a structural warning for every warning-level hazard in the stage's commands.
pub(crate) fn push_hazard_warnings(stage: &StageDefinition, warnings: &mut Vec<String>) {
    for (label, command, in_wiring_test) in stage_commands(stage) {
        for hazard in scan(command, in_wiring_test) {
            if !hazard.is_error() {
                let message = hazard.message();
                warnings.push(format!(
                    "Stage '{}': {label} `{command}` {message}",
                    stage.id
                ));
            }
        }
    }
}

/// Check whether an acceptance criterion invokes a resource that a worktree
/// session's sandboxed acceptance run cannot grant.
///
/// `loom map` and `loom knowledge context` both open `ContextStore`, which
/// resolves its cache under the main project root — read-only from a
/// worktree, and unreachable via `allow_write` since both settings emitters
/// filter out `../` paths. `tmux`/`docker` are host daemons the sandbox does
/// not expose. Returns the matched invocation for use in the warning
/// message, or `None` if the criterion is clean.
pub(super) fn criterion_needs_ungrantable_resource(cmd: &str) -> Option<&'static str> {
    if cmd.contains("loom map") {
        return Some("loom map");
    }
    if cmd.contains("loom knowledge context") {
        return Some("loom knowledge context");
    }
    for token in cmd.split_whitespace() {
        if token == "tmux" || token.ends_with("/tmux") {
            return Some("tmux");
        }
        if token == "docker" || token.ends_with("/docker") {
            return Some("docker");
        }
    }
    None
}

/// Every command the stage runs, labelled, with whether it is a wiring test.
fn stage_commands(stage: &StageDefinition) -> Vec<(String, &str, bool)> {
    let acceptance = stage.acceptance.iter().enumerate().map(|(idx, criterion)| {
        let label = format!("Acceptance criterion #{}", idx + 1);
        (label, criterion.command(), false)
    });
    let setup = stage.setup.iter().enumerate().map(|(idx, command)| {
        let label = format!("Setup command #{}", idx + 1);
        (label, command.as_str(), false)
    });
    let wiring = stage.wiring_tests.iter().enumerate().map(|(idx, test)| {
        let label = format!("Wiring test #{} '{}'", idx + 1, test.name);
        (label, test.command.as_str(), true)
    });
    acceptance.chain(setup).chain(wiring).collect()
}

/// Every hazard in `command`, including inside its command substitutions and
/// `sh -c` scripts.
pub(crate) fn scan(command: &str, in_wiring_test: bool) -> BTreeSet<Hazard> {
    let mut found = BTreeSet::new();
    scan_into(command, in_wiring_test, 0, true, &mut found);
    found
}

/// Scans one command, or a script nested inside it, for hazards. `tail`
/// marks a script whose own exit status becomes the exit status of the
/// criterion that ran it: the top-level command, or an `sh -c`/`bash -c`
/// script that is the last simple command of its parent. Only there does a
/// masked final statement change what the criterion reports.
fn scan_into(
    command: &str,
    in_wiring_test: bool,
    depth: usize,
    tail: bool,
    found: &mut BTreeSet<Hazard>,
) {
    let tokens = lex(command);
    if tail && masks_exit_status(&tokens) {
        found.insert(Hazard::MaskedExit);
    }
    let simples = simple_commands(&tokens);
    let last_index = simples.len().saturating_sub(1);
    for (idx, simple) in simples.iter().enumerate() {
        scan_simple(simple, in_wiring_test, found);
        if depth < MAX_NESTING {
            let script_tail = tail && idx == last_index;
            for script in nested_scripts(simple) {
                scan_into(&script, in_wiring_test, depth + 1, script_tail, found);
            }
        }
    }
}

fn scan_simple(simple: &SimpleCommand<'_>, in_wiring_test: bool, found: &mut BTreeSet<Hazard>) {
    let start = command_start(&simple.words);
    let argv = &simple.words[start..];
    let search = SearchArgs::parse(argv);
    let declares_names = argv
        .first()
        .is_some_and(|word| DECLARATION_BUILTINS.contains(&word.command_name()));
    for (idx, word) in argv.iter().enumerate() {
        if !search.as_ref().is_some_and(|args| args.is_pattern(idx)) {
            scan_word(word, declares_names && idx > 0, found);
        }
    }
    for word in &simple.words[..start] {
        scan_word(word, true, found);
    }
    for word in &simple.redirect_targets {
        scan_word(word, false, found);
    }
    found.extend(argv_hazard(argv, in_wiring_test));
    if search.is_some_and(|args| args.replaces()) {
        found.insert(Hazard::RgReplace);
    }
    if filters_vitest_by_name(argv) {
        found.insert(Hazard::VitestNameFilter);
    }
}

/// Hazards a single word carries. `is_assignment_position` marks a word in
/// the shell-assignment prefix before the command name (see
/// `command_start`), or an argument to a declaration builtin (see
/// `DECLARATION_BUILTINS`): only there does `NAME=value` set the shell/child
/// environment. The same shape elsewhere is an ordinary argument — e.g.
/// `awk -v HOME=$dir` sets an awk variable named `HOME`, not `$HOME`.
fn scan_word(word: &Word, is_assignment_position: bool, found: &mut BTreeSet<Hazard>) {
    if is_assignment_position && word.expands && word.assignment_name() == Some("HOME") {
        found.insert(Hazard::HomeFromExpansion);
    }
    if word.value.contains("doc/plans/") {
        found.insert(Hazard::PlanPath);
    }
    if word.expands && word.value.contains("PIPESTATUS") {
        found.insert(Hazard::PipeStatus);
    }
    if word.value.contains("/tmp/") && word.assignment_name() != Some("TMPDIR") {
        found.insert(Hazard::HardcodedTmp);
    }
}

/// Hazards decided by which command runs and with what arguments.
fn argv_hazard(argv: &[&Word], in_wiring_test: bool) -> Vec<Hazard> {
    let Some((first, args)) = argv.split_first() else {
        return Vec::new();
    };
    let name = first.command_name();
    let sub = args.first().map(|word| word.value.as_str());
    let mut hazards = Vec::new();
    let network = match (name, sub) {
        ("curl", _) => Some("curl"),
        ("wget", _) => Some("wget"),
        ("gh", _) => Some("gh"),
        ("npm", Some("install")) => Some("npm install"),
        ("bun", Some("install")) => Some("bun install"),
        ("cargo", Some("install")) => Some("cargo install"),
        ("cargo", Some("audit")) if !args.iter().any(|w| w.value == "--no-fetch") => {
            Some("cargo audit")
        }
        _ => None,
    };
    hazards.extend(network.map(Hazard::Network));
    if name == "mktemp" && is_bare_mktemp_dir(args) {
        hazards.push(Hazard::BareMktempDir);
    }
    if in_wiring_test && runs_test_runner(name, sub, argv) {
        hazards.push(Hazard::TestRunnerInWiring);
    }
    hazards
}

/// Index of the word that runs as the command, past assignments, keywords
/// and wrappers such as `env` or `timeout 60`.
pub(super) fn command_start(words: &[&Word]) -> usize {
    let mut idx = 0;
    let mut in_env = false;
    while let Some(word) = words.get(idx) {
        let value = word.value.as_str();
        let wrapper = !word.quoted && (KEYWORDS.contains(&value) || WRAPPERS.contains(&value));
        let env_option = in_env && value.starts_with('-');
        let skipped = wrapper || env_option || word.assignment_name().is_some();
        if !word.quoted && value == "timeout" {
            idx += 1;
            while words.get(idx).is_some_and(|w| w.value.starts_with('-')) {
                idx += 1;
            }
        } else if !skipped {
            break;
        }
        in_env |= value == "env";
        idx += 1;
    }
    idx.min(words.len())
}

fn is_bare_mktemp_dir(args: &[&Word]) -> bool {
    let directory = args.iter().any(|word| {
        let value = word.value.as_str();
        value == "--directory"
            || (value.starts_with('-') && !value.starts_with("--") && value.contains('d'))
    });
    let under_tmpdir = args
        .iter()
        .any(|word| word.value.starts_with("${TMPDIR:-/tmp}/"));
    directory && !under_tmpdir
}

/// Index of `tool` in `argv` when everything before it is a package runner.
fn launched(argv: &[&Word], tool: &str) -> Option<usize> {
    let idx = argv.iter().position(|word| word.command_name() == tool)?;
    let launchers_only = argv[..idx]
        .iter()
        .all(|word| LAUNCHERS.contains(&word.value.as_str()));
    launchers_only.then_some(idx)
}

fn runs_test_runner(name: &str, sub: Option<&str>, argv: &[&Word]) -> bool {
    let subcommand_runner = matches!(
        (name, sub),
        ("cargo", Some("test" | "nextest")) | ("bun" | "go", Some("test"))
    );
    let launched_runner = ["pytest", "vitest"]
        .into_iter()
        .any(|tool| launched(argv, tool).is_some());
    subcommand_runner || launched_runner
}

fn filters_vitest_by_name(argv: &[&Word]) -> bool {
    let Some(idx) = launched(argv, "vitest") else {
        return false;
    };
    argv[idx + 1..].iter().any(|word| {
        let value = word.value.as_str();
        value == "-t" || value.starts_with("-t=") || value.starts_with("--testNamePattern")
    })
}

/// Scripts nested in a simple command: its command substitutions, and the
/// script of an `sh -c` / `bash -c`.
pub(super) fn nested_scripts(simple: &SimpleCommand<'_>) -> Vec<String> {
    let mut scripts: Vec<String> = simple
        .words
        .iter()
        .chain(&simple.redirect_targets)
        .flat_map(|word| word.substitutions.iter().cloned())
        .collect();
    let argv = &simple.words[command_start(&simple.words)..];
    let is_shell = argv
        .first()
        .is_some_and(|word| matches!(word.command_name(), "sh" | "bash" | "zsh" | "dash"));
    if is_shell {
        let script_flag = argv.iter().position(|word| {
            let value = word.value.as_str();
            value.starts_with('-') && !value.starts_with("--") && value.ends_with('c')
        });
        if let Some(script) = script_flag.and_then(|idx| argv.get(idx + 1)) {
            scripts.push(script.value.clone());
        }
    }
    scripts
}
