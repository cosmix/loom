//! Dynamic shell completions for loom CLI.
//!
//! This module provides context-aware tab-completion for plan files,
//! stage IDs, session IDs, and knowledge files.

mod knowledge;
mod plans;
mod sessions;
mod stages;

#[cfg(test)]
mod tests;

use anyhow::Result;
use std::path::Path;

pub use knowledge::complete_knowledge_files;
pub use plans::complete_plan_files;
pub use sessions::{complete_session_ids, complete_stage_or_session_ids};
pub use stages::complete_stage_ids;

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
    ///
    /// # Returns
    ///
    /// A CompletionContext with parsed fields
    pub fn from_args(shell: &str, args: &[String]) -> Self {
        // Different shells pass arguments differently
        // bash: [cwd, cmdline, current_word, prev_word]
        // zsh: similar format
        // fish: may vary
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

/// Main entry point for dynamic completions
///
/// Determines what to complete based on context and prints results to stdout
///
/// # Arguments
///
/// * `ctx` - Completion context from shell
///
/// # Returns
///
/// Ok(()) on success, error if completion fails
pub fn complete_dynamic(ctx: &CompletionContext) -> Result<()> {
    let cwd = Path::new(&ctx.cwd);
    let prefix = &ctx.current_word;

    // Determine what to complete based on previous word and command line
    // Note: More specific guards must come BEFORE general matches
    let completions = match ctx.prev_word.as_str() {
        // Plan file completions
        "init" => complete_plan_files(cwd, prefix)?,

        // Session kill --stage flag completion (must come before general kill)
        "--stage" if ctx.cmdline.contains("sessions") && ctx.cmdline.contains("kill") => {
            complete_stage_ids(cwd, prefix)?
        }

        // Session kill with session IDs
        "kill" if ctx.cmdline.contains("sessions") => complete_session_ids(cwd, prefix)?,

        // Stage output subcommands (must check output context to avoid collision)
        "set" | "get" | "list" | "remove"
            if ctx.cmdline.contains("stage") && ctx.cmdline.contains("output") =>
        {
            complete_stage_ids(cwd, prefix)?
        }

        // Worktree remove (must come before general stage commands)
        "remove" if ctx.cmdline.contains("worktree") => complete_stage_ids(cwd, prefix)?,

        // Knowledge show/update file completions (must come before general stage commands)
        "show" | "update" if ctx.cmdline.contains("knowledge") => {
            complete_knowledge_files(prefix)?
        }

        // Stage subcommands that take stage_id (all in one pattern)
        "complete" | "block" | "reset" | "waiting" | "hold" | "release" | "skip" | "retry"
        | "recover" | "resume" | "verify" | "merge-complete"
            if ctx.cmdline.contains("stage") =>
        {
            complete_stage_ids(cwd, prefix)?
        }

        // Top-level commands that take stage_id (verify/merge/resume outside stage context)
        "verify" | "merge" | "resume" | "diagnose" => complete_stage_ids(cwd, prefix)?,

        _ => Vec::new(),
    };

    // Print completions, one per line
    for completion in completions {
        println!("{completion}");
    }

    Ok(())
}
