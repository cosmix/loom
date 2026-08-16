//! Confinement for plan-authored commands.
//!
//! Every acceptance criterion, setup command, truth check, wiring test,
//! dead-code check and change-impact command in a loom plan becomes a process
//! through [`spawn_confined`] — it is the single leaf primitive for that whole
//! family. Plans are trusted artifacts (see the trust model in [`super`]), but
//! trusted is not privileged: by default their commands run with a rebuilt,
//! allowlisted environment instead of the daemon's ambient one, so a plan line
//! cannot read `GITHUB_TOKEN`, `AWS_*` or `ANTHROPIC_API_KEY` merely because
//! loom happened to be started from a shell that had them.
//!
//! The module also owns the *policy* half — [`resolve_confinement`] and
//! [`plan_confinement`] answer "which level applies to this stage?" so no
//! caller has to reimplement the precedence.

use anyhow::{Context, Result};
use std::fmt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::models::stage::CommandConfinement;

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

/// Spawn a plan-authored command as a child process under `confinement`.
///
/// On Unix the child leads its own process group (pgid == child pid) so that a
/// timeout kill can reach grandchildren — `kill(-pgid, SIGKILL)` on a compound
/// `a && b` must also take down the `cargo test` it started.
///
/// stdin is `/dev/null` and both output streams are piped; callers must drain
/// them concurrently with waiting — `executor::run_spec_with_timeout` is the
/// one place that does so correctly.
pub fn spawn_confined(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    confinement: CommandConfinement,
) -> Result<Child> {
    let mut cmd = build_command(spec);

    match confinement {
        // Rebuild the child's environment from the documented host allowlist so
        // plan-authored commands never inherit loom's ambient credentials.
        CommandConfinement::Confined => crate::process::apply_stage_environment(&mut cmd),
        // Explicit plan opt-in: the command genuinely needs what loom was given.
        CommandConfinement::Inherit => {}
    }

    #[cfg(unix)]
    {
        // Place the child in its own process group so kill(-pgid, SIGKILL) on
        // timeout kills the entire subtree including grandchildren.
        // Safety: setpgid(0,0) is async-signal-safe per POSIX.
        unsafe {
            cmd.pre_exec(|| {
                nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                    .map_err(|e| std::io::Error::from_raw_os_error(e as i32))
            });
        }
    }

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    cmd.spawn()
        .with_context(|| format!("Failed to spawn command: {spec}"))
}

/// Effective confinement for one stage's plan-authored commands.
///
/// An explicit stage-level override wins over the plan-level default; with
/// neither set, commands are [`CommandConfinement::Confined`]. This is the
/// confinement-only projection of [`crate::sandbox::merge_config`]'s
/// stage-over-plan precedence, for the callers that hold the two values
/// without a full `SandboxConfig`.
pub fn resolve_confinement(
    stage_override: Option<CommandConfinement>,
    plan_default: Option<CommandConfinement>,
) -> CommandConfinement {
    stage_override.or(plan_default).unwrap_or_default()
}

/// Plan-level confinement default, read from the `[plan_sandbox]` snapshot
/// that `loom init`/`loom run` persist in `.work/config.toml`.
///
/// A missing or unreadable snapshot yields `None`, leaving callers at the
/// `Confined` default — ambiguity resolves toward the fail-safe level.
pub fn plan_confinement(work_dir: &Path) -> Option<CommandConfinement> {
    crate::fs::work_dir::read_plan_sandbox(work_dir)
        .ok()
        .flatten()
        .map(|sandbox| sandbox.command_confinement)
}

fn build_command(spec: &CommandSpec) -> Command {
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
