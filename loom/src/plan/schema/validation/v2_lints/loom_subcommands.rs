//! Unknown `loom` subcommands: a command naming a subcommand path the CLI does
//! not have exits with a usage error whatever the stage did, so it can never
//! pass. The CLI tree comes from `clap::CommandFactory`, the same source the
//! shell completions read.

use std::collections::HashMap;

use clap::{Command, CommandFactory};

use crate::plan::schema::structural_checks::transitive_dependencies;

use super::super::shell_lex::Word;
use super::{runs_loom, stage_touches_dir, visit_stage_argvs, LintContext, LintFinding};

/// Directory a plan can add a subcommand under; a stage that touches it, or
/// depends (transitively) on one that does, may be adding the very
/// subcommand it later calls.
const CLI_DIR: &str = "loom/src/cli";

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    let mut root = crate::cli::Cli::command();
    // Adds clap's generated `help` subcommand, which `loom help <cmd>` names.
    root.build();
    let stages = &ctx.metadata.loom.stages;
    let index_by_id: HashMap<&str, usize> = stages
        .iter()
        .enumerate()
        .map(|(idx, stage)| (stage.id.as_str(), idx))
        .collect();
    for (idx, stage) in stages.iter().enumerate() {
        // A stage runs on a base holding only its own changes and those of
        // its (transitive) dependencies, so an unrelated stage's reach into
        // `loom/src/cli` cannot excuse this stage's unknown subcommand - the
        // base this stage actually runs on never sees that stage's files.
        let adds_cli = stage_touches_dir(stage, CLI_DIR)
            || transitive_dependencies(idx, stages, &index_by_id)
                .into_iter()
                .any(|dep| stage_touches_dir(&stages[dep], CLI_DIR));
        visit_stage_argvs(stage, &mut |command, argv| {
            if let Some(problem) = unknown_subcommand(&root, argv) {
                let message = if adds_cli {
                    format!(
                        "{} (this stage or one it depends on changes `{CLI_DIR}`, so the \
                         subcommand may be one it adds)",
                        command.describe(&problem)
                    )
                } else {
                    command.describe(&problem)
                };
                out.push(LintFinding::in_stage(stage, message, !adds_cli));
            }
        });
    }
}

/// Walk the leading non-flag words of a `loom` argv down the CLI tree. Stops,
/// clean, at the first flag, at an expanded word, or at a subcommand that has
/// none of its own; reports a word no subcommand answers to, and a parent that
/// requires a subcommand but is given no more words.
fn unknown_subcommand(root: &Command, argv: &[&Word]) -> Option<String> {
    if !runs_loom(argv) {
        return None;
    }
    match walk(root, &argv[1..], false) {
        Walk::Clean => None,
        Walk::Unknown { path, word } => Some(format!(
            "runs `{path} {word}`, but `{word}` is not a subcommand of `{path}`, so it \
             fails with a usage error"
        )),
        Walk::MissingSubcommand { path } => Some(format!(
            "runs `{path}` without the subcommand it requires, so it fails with a usage error"
        )),
    }
}

/// Outcome of walking the words after `loom` down the CLI tree.
enum Walk {
    Clean,
    /// `word` is no subcommand of `path`.
    Unknown {
        path: String,
        word: String,
    },
    /// `path` is a parent that requires a subcommand and got none.
    MissingSubcommand {
        path: String,
    },
}

/// `placeholders` also stops, clean, at a doc placeholder word (`*`, `[`, `{`,
/// `<`, `…` or `...`).
fn walk(root: &Command, words: &[&Word], placeholders: bool) -> Walk {
    let mut current = root;
    let mut path = vec!["loom"];
    for word in words {
        let value = word.value.as_str();
        let stop = value.starts_with('-')
            || word.expands
            || !current.has_subcommands()
            || (placeholders && is_placeholder(value));
        if stop {
            return Walk::Clean;
        }
        match current.find_subcommand(value) {
            Some(subcommand) => {
                current = subcommand;
                path.push(value);
            }
            None if takes_free_words(current) => return Walk::Clean,
            None => {
                return Walk::Unknown {
                    path: path.join(" "),
                    word: value.to_string(),
                };
            }
        }
    }
    if current.has_subcommands() && current.is_subcommand_required_set() {
        return Walk::MissingSubcommand {
            path: path.join(" "),
        };
    }
    Walk::Clean
}

/// Whether a word that names no subcommand can still be a valid argument.
fn takes_free_words(command: &Command) -> bool {
    command.get_positionals().next().is_some() || command.is_allow_external_subcommands_set()
}

fn is_placeholder(value: &str) -> bool {
    value == "..." || value.contains(['*', '[', '{', '<', '\u{2026}'])
}

/// What `loom_command_paths` found in a script.
#[cfg(test)]
pub(crate) struct LoomCommandPaths {
    /// How many `loom` invocations the script holds.
    pub(crate) checked: usize,
    /// Each invocation's path up to and including a word no subcommand
    /// answers to, as `loom <word> ...`.
    pub(crate) unresolved: Vec<String>,
}

/// Resolve every `loom` invocation in `script` against the CLI tree. A family
/// named alone is accepted; doc placeholders end a path.
#[cfg(test)]
pub(crate) fn loom_command_paths(script: &str) -> LoomCommandPaths {
    let mut root = crate::cli::Cli::command();
    root.build();
    let mut found = LoomCommandPaths {
        checked: 0,
        unresolved: Vec::new(),
    };
    super::visit_argvs(script, 0, &mut |argv| {
        // `loom_run_bounded <n> loom ...` runs loom under a timeout.
        let argv = match argv.first() {
            Some(word) if word.value == "loom_run_bounded" => argv.get(2..).unwrap_or(&[]),
            _ => argv,
        };
        let Some((first, rest)) = argv.split_first() else {
            return;
        };
        let bin = matches!(
            first.value.as_str(),
            "${LOOM_BIN:-loom}" | "$LOOM_BIN" | "${LOOM_BIN}"
        );
        if !bin && first.command_name() != "loom" {
            return;
        }
        found.checked += 1;
        if let Walk::Unknown { path, word } = walk(&root, rest, true) {
            found.unresolved.push(format!("{path} {word}"));
        }
    });
    found
}
