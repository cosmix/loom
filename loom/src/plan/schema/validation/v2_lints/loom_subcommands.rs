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
    let mut current = root;
    let mut path = vec!["loom"];
    for word in &argv[1..] {
        let value = word.value.as_str();
        if value.starts_with('-') || word.expands || !current.has_subcommands() {
            return None;
        }
        match current.find_subcommand(value) {
            Some(subcommand) => {
                current = subcommand;
                path.push(value);
            }
            None if takes_free_words(current) => return None,
            None => {
                return Some(format!(
                    "runs `{} {value}`, but `{value}` is not a subcommand of `{}`, so it \
                     fails with a usage error",
                    path.join(" "),
                    path.join(" ")
                ));
            }
        }
    }
    let requires_subcommand = current.has_subcommands() && current.is_subcommand_required_set();
    requires_subcommand.then(|| {
        format!(
            "runs `{}` without the subcommand it requires, so it fails with a usage error",
            path.join(" ")
        )
    })
}

/// Whether a word that names no subcommand can still be a valid argument.
fn takes_free_words(command: &Command) -> bool {
    command.get_positionals().next().is_some() || command.is_allow_external_subcommands_set()
}
