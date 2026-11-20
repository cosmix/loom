//! What a confined command runs: a shell line or a program with arguments.

use std::fmt;
use std::process::Command;

/// Shell used to interpret a [`CommandSpec::Shell`] line.
#[cfg(unix)]
const SHELL: (&str, &str) = ("sh", "-c");
#[cfg(not(unix))]
const SHELL: (&str, &str) = ("cmd", "/C");

/// What to run.
///
/// `Shell` is the form plans are written in: one line handed to `sh -c`, so
/// `&&`, pipes and redirection work. `Program` is the typed form — a program
/// and its arguments, executed directly, with no shell to interpret
/// metacharacters in the arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandSpec {
    /// A shell line, executed via `sh -c` (Unix) or `cmd /C` (Windows).
    Shell(String),
    /// A program and its arguments, executed with no shell involved.
    Program {
        /// Executable to run, resolved through `PATH`.
        program: String,
        /// Arguments passed verbatim — never re-split, never expanded.
        args: Vec<String>,
    },
}

impl CommandSpec {
    /// Build a shell spec from a plan-authored command line.
    pub fn shell(command: impl Into<String>) -> Self {
        Self::Shell(command.into())
    }

    /// Build a shell-free spec from a program and its arguments.
    pub fn program<I, S>(program: impl Into<String>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Program {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

impl fmt::Display for CommandSpec {
    /// Renders the spec for error context and result records. This is a
    /// human-readable rendering, not a re-runnable shell line.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shell(command) => f.write_str(command),
            Self::Program { program, args } => {
                f.write_str(program)?;
                for arg in args {
                    write!(f, " {arg}")?;
                }
                Ok(())
            }
        }
    }
}

pub(super) fn build_command(spec: &CommandSpec) -> Command {
    match spec {
        CommandSpec::Shell(line) => {
            // The line is passed as a single argument: the shell interprets it,
            // but loom never splits it into arguments itself.
            let (shell, flag) = SHELL;
            let mut command = Command::new(shell);
            command.arg(flag).arg(line);
            command
        }
        CommandSpec::Program { program, args } => {
            let mut command = Command::new(program);
            command.args(args);
            command
        }
    }
}
