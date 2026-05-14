//! Dynamic shell completions for loom CLI.
//!
//! This module provides context-aware tab-completion for commands,
//! subcommands, flags, plan files, stage IDs, session IDs, knowledge
//! files, and memory entry types.

mod commands;
mod knowledge;
mod memory;
mod plans;
mod sessions;
mod stages;

#[cfg(test)]
mod tests;

use anyhow::Result;
use std::path::Path;

pub use commands::{
    complete_commands, complete_flags, complete_model_names, complete_shell_types,
    complete_subcommands, complete_trigger_types,
};
pub use knowledge::complete_knowledge_files;
pub use memory::complete_memory_entry_types;
pub use plans::complete_plan_files;
pub use sessions::{complete_session_ids, complete_stage_or_session_ids};
pub use stages::{complete_stage_ids, complete_stage_ids_filtered};

/// Context for shell completion
#[derive(Debug, Clone)]
pub struct CompletionContext {
    pub cwd: String,
    pub shell: String,
    pub cmdline: String,
    pub current_word: String,
    pub prev_word: String,
}

impl CompletionContext {
    /// Parse completion context from shell-provided arguments
    ///
    /// # Arguments
    ///
    /// * `shell` - Shell type (bash, zsh, fish)
    /// * `args` - Arguments passed from shell completion system
    pub fn from_args(shell: &str, args: &[String]) -> Self {
        let cwd = args.first().cloned().unwrap_or_else(|| ".".to_string());
        let cmdline = args.get(1).cloned().unwrap_or_default();
        let current_word = args.get(2).cloned().unwrap_or_default();
        let prev_word = args.get(3).cloned().unwrap_or_default();

        Self {
            cwd,
            shell: shell.to_string(),
            cmdline,
            current_word,
            prev_word,
        }
    }
}

/// Parse the command line into words, stripping the leading "loom" if present.
fn parse_cmdline_words(cmdline: &str) -> Vec<String> {
    let words: Vec<String> = cmdline.split_whitespace().map(String::from).collect();
    if words.first().map(|w| w.as_str()) == Some("loom") {
        words[1..].to_vec()
    } else {
        words
    }
}

/// Extract the command path (non-flag words) from parsed words,
/// excluding the current word being completed.
fn extract_command_path<'a>(words: &'a [String], current_word: &str) -> Vec<&'a str> {
    words
        .iter()
        .filter(|w| !w.starts_with('-'))
        .filter(|w| w.as_str() != current_word || current_word.is_empty())
        .map(|w| w.as_str())
        .collect()
}

/// Main entry point for dynamic completions.
///
/// Determines what to complete based on context and prints results to stdout.
pub fn complete_dynamic(ctx: &CompletionContext) -> Result<()> {
    let cwd = Path::new(&ctx.cwd);
    let prefix = &ctx.current_word;
    let words = parse_cmdline_words(&ctx.cmdline);
    let cmd_path = extract_command_path(&words, prefix);

    let completions = route_completion(cwd, prefix, &ctx.prev_word, &cmd_path, &ctx.cmdline)?;

    for completion in completions {
        println!("{completion}");
    }

    Ok(())
}

/// Route to the appropriate completion function based on context.
fn route_completion(
    cwd: &Path,
    prefix: &str,
    prev_word: &str,
    cmd_path: &[&str],
    cmdline: &str,
) -> Result<Vec<String>> {
    // If current word starts with -, complete flags based on command path
    if prefix.starts_with('-') {
        return complete_flags(cmd_path, prefix);
    }

    // Check if prev_word is a flag that expects a value
    if let Some(completions) = complete_flag_value(cwd, prefix, prev_word, cmd_path, cmdline)? {
        return Ok(completions);
    }

    // Route based on command path depth
    match cmd_path {
        [] => complete_commands(prefix),
        [cmd] => complete_after_command(cwd, prefix, cmd),
        [cmd, sub] => complete_after_subcommand(cwd, prefix, cmd, sub),
        [_cmd, "output", sub] => complete_after_subcommand(cwd, prefix, "output", sub),
        _ => Ok(Vec::new()),
    }
}

/// Complete values after a flag that expects an argument.
fn complete_flag_value(
    cwd: &Path,
    prefix: &str,
    prev_word: &str,
    _cmd_path: &[&str],
    cmdline: &str,
) -> Result<Option<Vec<String>>> {
    match prev_word {
        "--stage" => {
            let results = complete_stage_ids(cwd, prefix)?;
            Ok(Some(results))
        }
        "--entry-type" | "-t" if cmdline.contains("memory") => {
            let results = complete_memory_entry_types(prefix)?;
            Ok(Some(results))
        }
        "--model" => {
            let results = complete_model_names(prefix)?;
            Ok(Some(results))
        }
        "--trigger" => {
            let results = complete_trigger_types(prefix)?;
            Ok(Some(results))
        }
        "--session" => {
            let results = complete_session_ids(cwd, prefix)?;
            Ok(Some(results))
        }
        // --tail expects a number; suppress stage-ID suggestions
        "--tail" => Ok(Some(Vec::new())),
        _ => Ok(None),
    }
}

/// Complete after a single top-level command.
fn complete_after_command(cwd: &Path, prefix: &str, cmd: &str) -> Result<Vec<String>> {
    if commands::has_subcommands(cmd) {
        return complete_subcommands(cmd, prefix);
    }

    match cmd {
        // init takes any file path — return empty so the shell falls back
        // to native path completion
        "init" => Ok(Vec::new()),
        "completions" => complete_shell_types(prefix),
        "check" | "diagnose" | "resume" => complete_stage_ids(cwd, prefix),
        _ => Ok(Vec::new()),
    }
}

/// Complete after a subcommand (two-level depth).
fn complete_after_subcommand(
    cwd: &Path,
    prefix: &str,
    cmd: &str,
    sub: &str,
) -> Result<Vec<String>> {
    match (cmd, sub) {
        // Stage subcommands that take stage IDs
        ("stage", "complete") => complete_stage_ids_filtered(cwd, prefix, &["executing"]),
        ("stage", "retry") => complete_stage_ids_filtered(
            cwd,
            prefix,
            &[
                "blocked",
                "completed-with-failures",
                "merge-blocked",
                "needs-handoff",
            ],
        ),
        ("stage", "merge") => {
            complete_stage_ids_filtered(cwd, prefix, &["merge-conflict", "merge-blocked"])
        }
        ("stage", "reset") => complete_stage_ids_filtered(
            cwd,
            prefix,
            &[
                "blocked",
                "executing",
                "completed-with-failures",
                "needs-handoff",
            ],
        ),
        ("stage", "verify") => {
            complete_stage_ids_filtered(cwd, prefix, &["completed-with-failures", "executing"])
        }
        ("stage", "human-review" | "dispute-criteria") => complete_stage_ids(cwd, prefix),
        ("stage", "block" | "hold" | "release" | "skip" | "waiting" | "resume") => {
            complete_stage_ids(cwd, prefix)
        }
        ("stage", "output") => complete_subcommands("output", prefix),

        // Output subcommands take stage IDs
        ("output", "set" | "get" | "list" | "remove") => complete_stage_ids(cwd, prefix),

        // Session subcommands
        ("sessions", "kill") => complete_session_ids(cwd, prefix),

        // Worktree subcommands
        ("worktree", "remove") => complete_stage_ids(cwd, prefix),

        // Knowledge subcommands
        ("knowledge", "show" | "update") => complete_knowledge_files(prefix),

        // Plan subcommands
        ("plan", "verify") => complete_plan_files(cwd, prefix),

        _ => Ok(Vec::new()),
    }
}
