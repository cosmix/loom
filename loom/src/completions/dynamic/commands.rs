//! Command, subcommand, flag, and static value completions.

use anyhow::Result;
use clap::{Command, CommandFactory};

fn command_matches(command: &Command, name: &str) -> bool {
    command.get_name() == name || command.get_all_aliases().any(|alias| alias == name)
}

fn command_at_path<'a>(root: &'a Command, path: &[&str]) -> Option<&'a Command> {
    let mut current = root;
    for segment in path {
        current = current
            .get_subcommands()
            .find(|command| command_matches(command, segment))?;
    }
    Some(current)
}

fn find_command_named<'a>(root: &'a Command, name: &str) -> Option<&'a Command> {
    root.get_subcommands().find_map(|command| {
        command_matches(command, name)
            .then_some(command)
            .or_else(|| find_command_named(command, name))
    })
}

fn visible_subcommand_names(command: &Command) -> Vec<String> {
    let mut names: Vec<String> = command
        .get_subcommands()
        .filter(|subcommand| !subcommand.is_hide_set())
        .map(|subcommand| subcommand.get_name().to_string())
        .collect();
    names.sort();
    names
}

/// Complete top-level command names.
pub fn complete_commands(prefix: &str) -> Result<Vec<String>> {
    let root = crate::cli::Cli::command();
    Ok(filter_owned(visible_subcommand_names(&root), prefix))
}

/// Complete subcommands for a parent command.
pub fn complete_subcommands(parent: &str, prefix: &str) -> Result<Vec<String>> {
    let root = crate::cli::Cli::command();
    let Some(command) = find_command_named(&root, parent) else {
        return Ok(Vec::new());
    };
    Ok(filter_owned(visible_subcommand_names(command), prefix))
}

/// Complete flags for a given command path.
///
/// `command_path` is a slice of command words, e.g. `["stage", "complete"]`.
pub fn complete_flags(command_path: &[&str], prefix: &str) -> Result<Vec<String>> {
    let root = crate::cli::Cli::command();
    let Some(command) = command_at_path(&root, command_path) else {
        return Ok(Vec::new());
    };
    let mut flags = Vec::new();
    for argument in command.get_arguments().filter(|arg| !arg.is_hide_set()) {
        if let Some(long) = argument.get_long() {
            flags.push(format!("--{long}"));
        }
        if let Some(short) = argument.get_short() {
            flags.push(format!("-{short}"));
        }
    }
    flags.sort();
    flags.dedup();
    Ok(filter_owned(flags, prefix))
}

/// Complete the values of the argument `arg_id` of the command at
/// `command_path`, from the values clap lists for it (a `ValueEnum`'s names).
pub fn complete_flag_choices(
    command_path: &[&str],
    arg_id: &str,
    prefix: &str,
) -> Result<Vec<String>> {
    let root = crate::cli::Cli::command();
    let argument = command_at_path(&root, command_path).and_then(|command| {
        command
            .get_arguments()
            .find(|argument| argument.get_id() == arg_id)
    });
    let names = argument
        .map(|argument| argument.get_possible_values())
        .unwrap_or_default()
        .iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| value.get_name().to_string())
        .collect();
    Ok(filter_owned(names, prefix))
}

/// Complete shell type names (bash, zsh, fish).
pub fn complete_shell_types(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(&["bash", "fish", "zsh"], prefix))
}

/// Complete Claude model names for --model/--claude-model/--address-model flags.
pub fn complete_model_names(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(crate::claude::CLAUDE_MODELS, prefix))
}

/// Complete Codex model names for the --codex-model flag.
pub fn complete_codex_model_names(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(crate::codex::CODEX_MODELS, prefix))
}

/// Complete handoff trigger types for --trigger flag.
pub fn complete_trigger_types(prefix: &str) -> Result<Vec<String>> {
    Ok(filter_prefix(
        &["manual", "precompact", "session_end"],
        prefix,
    ))
}

/// Commands that have subcommands (and thus should not get value completions).
pub fn has_subcommands(command: &str) -> bool {
    let root = crate::cli::Cli::command();
    find_command_named(&root, command).is_some_and(|command| command.has_subcommands())
}

/// Filter a list of candidates by prefix.
fn filter_prefix(candidates: &[&str], prefix: &str) -> Vec<String> {
    candidates
        .iter()
        .filter(|c| prefix.is_empty() || c.starts_with(prefix))
        .map(|s| s.to_string())
        .collect()
}

fn filter_owned(candidates: Vec<String>, prefix: &str) -> Vec<String> {
    candidates
        .into_iter()
        .filter(|candidate| prefix.is_empty() || candidate.starts_with(prefix))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::complete_flags;

    #[test]
    fn run_completions_never_offer_the_hidden_daemon_child_flag() {
        let hidden = complete_flags(&["run"], "--daemon").unwrap();
        assert!(
            !hidden.contains(&"--daemon-child".to_string()),
            "{hidden:?}"
        );

        let all = complete_flags(&["run"], "--").unwrap();
        assert!(all.contains(&"--manual".to_string()), "{all:?}");
    }
}
