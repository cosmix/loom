//! Unknown `loom` subcommands: a command naming a subcommand path the CLI does
//! not have exits with a usage error whatever the stage did, so it can never
//! pass. The CLI tree comes from `clap::CommandFactory`, the same source the
//! shell completions read.

use clap::{Command, CommandFactory};

use super::super::shell_lex::Word;
use super::{runs_loom, visit_stage_argvs, LintContext, LintFinding};

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    let mut root = crate::cli::Cli::command();
    // Adds clap's generated `help` subcommand, which `loom help <cmd>` names.
    root.build();
    for stage in &ctx.metadata.loom.stages {
        visit_stage_argvs(stage, &mut |command, argv| {
            if let Some(problem) = unknown_subcommand(&root, argv) {
                out.push(LintFinding::in_stage(
                    stage,
                    command.describe(&problem),
                    true,
                ));
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
