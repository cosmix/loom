//! Package installs in a plan `provision` command must be hardened: provision runs
//! with network access in a worktree a stage can edit, so an install that runs
//! lifecycle scripts or reads repository config runs code the stage wrote.

use super::super::shell_lex::{lex, simple_commands, Token, Word};
use super::super::v2_lints::visit_argvs;

/// `$install` behind the refusal of a worktree `.npmrc`: an agent-written one
/// redirects the registry even when every flag below is passed, and `-L` also
/// catches a dangling symlink, which `-e` reads as absent.
macro_rules! refusing_npmrc {
    ($install:literal) => {
        concat!("test ! -e .npmrc && test ! -L .npmrc && ", $install)
    };
}

/// The hardened installs validation accepts; the JS provision lint suggests the
/// first four.
pub(crate) const BUN_INSTALL: &str = refusing_npmrc!(
    "bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
);
pub(crate) const PNPM_INSTALL: &str =
    refusing_npmrc!("pnpm install --frozen-lockfile --ignore-scripts --ignore-pnpmfile");
pub(crate) const YARN_INSTALL: &str =
    refusing_npmrc!("yarn install --frozen-lockfile --ignore-scripts");
pub(crate) const NPM_INSTALL: &str = refusing_npmrc!("npm ci --ignore-scripts");
const UV_SYNC: &str = "uv sync --frozen --no-install-project";

const LIFECYCLE_WHY: &str =
    "package lifecycle scripts and an agent-written `.npmrc` would act on the host";

const NPMRC_REFUSAL: &str = "a leading `test ! -e .npmrc && test ! -L .npmrc &&` that joins the \
                             rest with `&&` only, in the install's own directory with no `cd`";

/// A package install a provision command may run, and what keeps the repository's
/// own code from running on the host.
struct InstallRule {
    tool: &'static str,
    /// Subcommands that install; `""` stands for the bare tool (`yarn`).
    subcommands: &'static [&'static str],
    /// Flags the install must pass; `--name=value` also matches `--name value`.
    flags: &'static [&'static str],
    /// Whether the installer reads a worktree `.npmrc`.
    reads_npmrc: bool,
    why: &'static str,
    hardened: &'static str,
}

static INSTALL_RULES: [InstallRule; 5] = [
    InstallRule {
        tool: "bun",
        subcommands: &["install", "i"],
        flags: &[
            "--ignore-scripts",
            "--backend=copyfile",
            "--config=/dev/null",
        ],
        reads_npmrc: true,
        why: "package lifecycle scripts, an agent-written `bunfig.toml` or `.npmrc` and \
              hardlinks into the shared bun cache would act on the host",
        hardened: BUN_INSTALL,
    },
    InstallRule {
        tool: "npm",
        subcommands: &["ci", "install", "i"],
        flags: &["--ignore-scripts"],
        reads_npmrc: true,
        why: LIFECYCLE_WHY,
        hardened: NPM_INSTALL,
    },
    InstallRule {
        tool: "pnpm",
        subcommands: &["install", "i"],
        flags: &["--ignore-scripts", "--ignore-pnpmfile"],
        reads_npmrc: true,
        why: "package lifecycle scripts, the repository's `.pnpmfile.cjs` and an \
              agent-written `.npmrc` would act on the host",
        hardened: PNPM_INSTALL,
    },
    InstallRule {
        tool: "yarn",
        subcommands: &["", "install"],
        flags: &["--ignore-scripts"],
        reads_npmrc: true,
        why: LIFECYCLE_WHY,
        hardened: YARN_INSTALL,
    },
    InstallRule {
        tool: "uv",
        subcommands: &["sync"],
        flags: &["--no-install-project"],
        reads_npmrc: false,
        why: "uv would build the local project through its build backend, which runs \
              repository code",
        hardened: UV_SYNC,
    },
];

/// One message per package install in `command` that lacks a flag, or the `.npmrc`
/// refusal, its installer needs ([`INSTALL_RULES`]). `label` names the entry; the
/// working dir is never printed.
pub(super) fn push_install_problems(label: &str, command: &str, messages: &mut Vec<String>) {
    let refuses_npmrc = refuses_npmrc_first(command);
    visit_argvs(command, 0, &mut |argv| {
        let Some((rule, install)) = install_rule(argv) else {
            return;
        };
        let args: Vec<&str> = argv[1..].iter().map(|word| word.value.as_str()).collect();
        let mut missing: Vec<String> = rule
            .flags
            .iter()
            .filter(|flag| !passes(&args, flag))
            .map(|flag| format!("`{flag}`"))
            .collect();
        if rule.reads_npmrc && !refuses_npmrc {
            missing.push(NPMRC_REFUSAL.to_string());
        }
        if let Some((last, rest)) = missing.split_last() {
            let listed = if rest.is_empty() {
                last.clone()
            } else {
                format!("{} and {last}", rest.join(", "))
            };
            messages.push(format!(
                "{label} runs `{install}` without {listed}: provision runs on the host in a \
                 worktree a stage can edit, so {}; use `{}`",
                rule.why, rule.hardened
            ));
        }
    });
}

/// The install rule `argv` falls under, and the install as the message names it
/// (`<tool> <subcommand>`, or the bare tool).
fn install_rule(argv: &[&Word]) -> Option<(&'static InstallRule, String)> {
    let (first, rest) = argv.split_first()?;
    let tool = first.command_name();
    let sub = rest
        .iter()
        .map(|word| word.value.as_str())
        .find(|arg| !arg.starts_with('-'));
    let rule = INSTALL_RULES
        .iter()
        .find(|rule| rule.tool == tool && rule.subcommands.contains(&sub.unwrap_or("")))?;
    let install = sub.map_or_else(|| tool.to_string(), |sub| format!("{tool} {sub}"));
    Some((rule, install))
}

/// Whether `args` pass `flag`, either as written or, for `--name=value`, as
/// `--name value`.
fn passes(args: &[&str], flag: &str) -> bool {
    let spaced = flag
        .split_once('=')
        .is_some_and(|(name, value)| args.windows(2).any(|pair| pair == [name, value]));
    spaced || args.contains(&flag)
}

/// Whether `command` opens with `test ! -e .npmrc` and `test ! -L .npmrc` (either
/// order) and joins every command with `&&`, so nothing after the refusal runs
/// while the worktree holds a `.npmrc`. The refusal only covers the install's own
/// directory, so a command that changes directory (`cd`, `pushd`, `popd`, also
/// inside an `sh -c` script) never counts: the install could run where the
/// refusal did not look.
fn refuses_npmrc_first(command: &str) -> bool {
    let tokens = lex(command);
    let commands = simple_commands(&tokens);
    let refuses = |idx: usize, flag: &str| {
        commands.get(idx).is_some_and(|simple| {
            let values = simple.words.iter().map(|word| word.value.as_str());
            values.eq(["test", "!", flag, ".npmrc"])
        })
    };
    let opens = (refuses(0, "-e") && refuses(1, "-L")) || (refuses(0, "-L") && refuses(1, "-e"));
    opens && joined_by_and(&tokens) && !changes_directory(command)
}

/// Whether any command `command` runs, nested `sh -c` scripts included, is `cd`,
/// `pushd` or `popd`.
fn changes_directory(command: &str) -> bool {
    let mut changes = false;
    visit_argvs(command, 0, &mut |argv| {
        changes |= matches!(argv[0].command_name(), "cd" | "pushd" | "popd");
    });
    changes
}

/// Whether every control operator in `tokens` is `&&` or a grouping parenthesis.
/// A newline lexes as `;`: after `&&` it continues the line and at the end it ends
/// the command, so both are allowed; any other `;`, `||`, `|` or `&` would let a
/// command run whether or not the ones before it succeeded.
fn joined_by_and(tokens: &[Token]) -> bool {
    let newline = Token::Control(";");
    let end = tokens
        .iter()
        .rposition(|token| *token != newline)
        .map_or(0, |idx| idx + 1);
    let mut after_and = false;
    tokens[..end].iter().all(|token| {
        let allowed = match token {
            Token::Control("&&") => true,
            Token::Control(";") => after_and,
            Token::Control(op) => matches!(*op, "(" | ")"),
            _ => true,
        };
        after_and = allowed && matches!(token, Token::Control("&&" | ";"));
        allowed
    })
}
