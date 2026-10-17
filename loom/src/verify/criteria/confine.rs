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

#[path = "confine_host.rs"]
mod host;

use anyhow::{Context, Result};
use std::fmt;
use std::path::{Path, PathBuf};
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

/// A one-shot command whose environment and working directory are already
/// fixed. Cache fingerprinting records its stable inputs and wrapper policy;
/// only the generated private tool-state pathname is excluded from identity.
pub(super) struct PreparedCommand {
    command: Command,
    working_dir: Option<PathBuf>,
    display: String,
    spec: CommandSpec,
    wrapped: Option<Command>,
    scratch: Option<host::CargoScratch>,
}

impl PreparedCommand {
    // This is the stable input environment. The wrapper maps its Cargo home to
    // a private generated directory under the versioned containment policy.
    pub(super) fn command(&self) -> &Command {
        &self.command
    }

    pub(super) fn working_dir(&self) -> Option<&Path> {
        self.working_dir.as_deref()
    }

    pub(super) fn display(&self) -> &str {
        &self.display
    }

    pub(super) fn spec(&self) -> &CommandSpec {
        &self.spec
    }

    pub(super) fn containment_executable(&self) -> Option<PathBuf> {
        self.wrapped
            .as_ref()
            .map(|command| PathBuf::from(command.get_program()))
    }

    pub(super) fn containment_policy_revision(&self) -> String {
        let arguments = self
            .wrapped
            .as_ref()
            .map(|command| command.get_args().collect::<Vec<_>>());
        let revision = format!("host-os-boundary-v6:{arguments:?}");
        match &self.scratch {
            Some(scratch) => revision.replace(
                scratch.path().to_string_lossy().as_ref(),
                "$LOOM_COMMAND_SCRATCH",
            ),
            None => revision,
        }
    }

    pub(super) fn spawn(mut self) -> Result<ConfinedChild> {
        let child = self.spawn_retaining_resources()?;
        Ok(ConfinedChild {
            process: crate::process::OwnedProcess::new(child),
            _scratch: self.scratch.take(),
        })
    }

    pub(super) fn spawn_retaining_resources(&mut self) -> Result<Child> {
        if let Some(scratch) = &self.scratch {
            scratch.populate()?;
        }
        if let Some(wrapped) = self.wrapped.take() {
            self.command = wrapped;
        }
        self.command
            .spawn()
            .with_context(|| format!("Failed to spawn command: {}", self.display))
    }
}

/// An owned confined child: dropping it retires its process group before
/// removing its private tool state. Borrow the child for pipe and wait APIs.
pub struct ConfinedChild {
    process: crate::process::OwnedProcess,
    _scratch: Option<host::CargoScratch>,
}

impl fmt::Debug for ConfinedChild {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConfinedChild").finish_non_exhaustive()
    }
}

impl ConfinedChild {
    pub fn child(&mut self) -> &mut Child {
        self.process.child()
    }
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
) -> Result<ConfinedChild> {
    prepare_confined(spec, working_dir, confinement)?.spawn()
}

/// Resolve a confined command once, without spawning it yet.
pub(super) fn prepare_confined(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    confinement: CommandConfinement,
) -> Result<PreparedCommand> {
    prepare(spec, working_dir, confinement, host::NetworkAccess::Denied)
}

/// Provision entries are approved package setup: retain the command boundary
/// while permitting the network needed to fetch dependencies.
pub(super) fn prepare_provision(spec: &CommandSpec, working_dir: &Path) -> Result<PreparedCommand> {
    prepare(
        spec,
        Some(working_dir),
        CommandConfinement::Confined,
        host::NetworkAccess::Provision,
    )
}

fn prepare(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    confinement: CommandConfinement,
    network: host::NetworkAccess,
) -> Result<PreparedCommand> {
    let mut cmd = build_command(spec);

    match confinement {
        // Rebuild the child's environment from the documented host allowlist so
        // plan-authored commands never inherit loom's ambient credentials.
        CommandConfinement::Confined => apply_confined_environment(&mut cmd)?,
        // Explicit plan opt-in: the command genuinely needs what loom was given.
        CommandConfinement::Inherit => {}
    }
    #[cfg(unix)]
    cmd.process_group(0);

    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let resolved_working_dir = working_dir
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok());
    if let Some(dir) = &resolved_working_dir {
        cmd.current_dir(dir);
        if confinement == CommandConfinement::Confined {
            cmd.env("TMPDIR", dir).env("TMP", dir).env("TEMP", dir);
        }
    }
    let (wrapped, scratch) = if confinement == CommandConfinement::Confined {
        let (wrapped, scratch) = host::wrap(&cmd, network)
            .with_context(|| format!("Failed to confine command: {spec}"))?;
        (Some(wrapped), Some(scratch))
    } else {
        (None, None)
    };
    Ok(PreparedCommand {
        command: cmd,
        working_dir: resolved_working_dir,
        display: spec.to_string(),
        spec: spec.clone(),
        wrapped,
        scratch,
    })
}

/// The running executable is a runtime dependency of `loom ...` criteria.
/// Derive it from this process, never from a caller-supplied `LOOM_BIN`.
fn apply_confined_environment(command: &mut Command) -> Result<()> {
    crate::process::apply_stage_environment(command);
    let executable = std::env::current_exe().context("resolve the running Loom executable")?;
    let directory = executable
        .parent()
        .context("Loom executable has no parent directory")?;
    let path = command
        .get_envs()
        .find_map(|(key, value)| (key == "PATH").then_some(value).flatten())
        .unwrap_or_else(|| std::ffi::OsStr::new("/usr/bin:/bin"));
    let directories = std::iter::once(directory.to_path_buf())
        .chain(std::env::split_paths(path).filter(|entry| entry != directory));
    let path = std::env::join_paths(directories).context("build the confined command PATH")?;
    command.env("PATH", path).env("LOOM_BIN", executable);
    Ok(())
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
/// that `loom init`/`loom run` persist in `.loom/work/config.toml`.
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
