//! Commands for creating and inspecting planning summaries.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::cli::SummaryCommands;
use crate::context::untrusted::terminal_safe;
use crate::planning::runtime::{runtime_paths, RuntimePaths};
use crate::planning::summary::brief::brief;
use crate::planning::summary::seal::{seal, SealError};
use crate::planning::summary::{claims, format::FormatError, status};

#[cfg(test)]
mod tests;

#[derive(Debug)]
enum CommandError {
    Rejected(String),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for CommandError {
    fn from(error: anyhow::Error) -> Self {
        Self::Other(error)
    }
}

fn exit_code(error: &CommandError) -> i32 {
    match error {
        CommandError::Rejected(_) => 1,
        CommandError::Other(_) => 2,
    }
}

fn draft_errors(path: &Path, errors: &[FormatError]) -> String {
    errors
        .iter()
        .map(|error| {
            terminal_safe(&format!(
                "{}:{}: {}",
                path.display(),
                error.line,
                error.message
            ))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn claim_error(error: anyhow::Error) -> CommandError {
    if error.chain().any(|cause| cause.is::<claims::ClaimHeld>()) {
        CommandError::Rejected(terminal_safe(&format!("{error:#}")))
    } else {
        CommandError::Other(error)
    }
}

fn root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("cannot determine current directory")?;
    // `run_git_records` keeps the output untrimmed: a repository path may begin or end with a
    // space, which `run_git_checked` would strip.
    let output = crate::git::runner::run_git_records(&["rev-parse", "--show-toplevel"], &cwd)
        .with_context(|| format!("{} is not inside a git repository", cwd.display()))?;
    Ok(PathBuf::from(output.trim_end_matches('\n')))
}

fn run(command: SummaryCommands, root: &Path, paths: &RuntimePaths) -> Result<(), CommandError> {
    match command {
        SummaryCommands::Seal { id, force } => match seal(root, paths, &id, force) {
            Ok(outcome) => println!(
                "Sealed {} ({} sentences)",
                terminal_safe(&outcome.path.display().to_string()),
                outcome.sentences
            ),
            Err(SealError::Rejected(errors)) => {
                let draft = claims::draft_path(paths, &id);
                let draft = if draft.is_absolute() {
                    draft
                } else {
                    std::env::current_dir()
                        .map_err(anyhow::Error::from)?
                        .join(draft)
                };
                return Err(CommandError::Rejected(draft_errors(&draft, &errors)));
            }
            Err(error) => return Err(CommandError::Other(anyhow::anyhow!(error))),
        },
        SummaryCommands::Brief { ids, note } => {
            print!(
                "{}",
                brief(root, paths, &ids, note.as_deref()).map_err(claim_error)?
            );
        }
        SummaryCommands::Status { json } => {
            let report = status::status_report(root, paths)?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).map_err(anyhow::Error::from)?
                );
            } else {
                print!("{}", status::render_status(&report));
            }
        }
        SummaryCommands::Release { ids } => {
            for id in claims::release(paths, &ids).map_err(claim_error)? {
                println!("Released {}", terminal_safe(&id));
            }
        }
    }
    Ok(())
}

/// Execute a `loom plan summary` subcommand.
pub fn execute(command: SummaryCommands) -> Result<()> {
    let result = (|| {
        let root = root()?;
        let paths = runtime_paths(&root)?;
        run(command, &root, &paths)
    })();
    if let Err(error) = result {
        let code = exit_code(&error);
        match error {
            CommandError::Rejected(message) => eprintln!("{message}"),
            CommandError::Other(error) => eprintln!(
                "loom plan summary: {}",
                terminal_safe(&format!("{error:#}"))
            ),
        }
        std::process::exit(code);
    }
    Ok(())
}
