//! Dynamic shell completions for loom CLI.
//!
//! This module provides context-aware tab-completion for plan files,
//! stage IDs, and session IDs.

mod plans;
mod sessions;
mod stages;

#[cfg(test)]
mod tests;

use anyhow::Result;
use std::path::Path;

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
    let completions = match ctx.prev_word.as_str() {
        "init" => complete_plan_files(cwd, prefix)?,

        "verify" | "merge" | "resume" => complete_stage_ids(cwd, prefix)?,

        "attach" => complete_stage_or_session_ids(cwd, prefix)?,

        "kill" if ctx.cmdline.contains("sessions") => complete_session_ids(cwd, prefix)?,

        "complete" | "block" | "reset" | "waiting" if ctx.cmdline.contains("stage") => {
            complete_stage_ids(cwd, prefix)?
        }

        _ => Vec::new(),
    };

    // Print completions, one per line
    for completion in completions {
        println!("{completion}");
    }

    Ok(())
}
