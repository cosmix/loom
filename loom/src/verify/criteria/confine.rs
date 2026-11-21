//! Confinement for plan-authored commands.
//!
//! Every acceptance criterion, setup command, truth check, wiring test and
//! dead-code check in a loom plan becomes a process
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
pub(super) mod host;
#[cfg(test)]
#[path = "confine_policy.rs"]
mod policy;
#[path = "confine_scratch.rs"]
mod scratch;
#[path = "confine_spec.rs"]
mod spec;

use host::wrap;
use spec::build_command;
pub use spec::CommandSpec;

#[cfg(target_os = "linux")]
pub(crate) use host::git_roots::git_dirs;
pub(super) use host::git_roots::GitDirs;
#[cfg(target_os = "linux")]
pub(crate) use scratch::CargoScratch;

#[cfg(test)]
pub(crate) use policy::{confined_policy, ConfinedPolicy};

/// The OS sandbox launcher tests need, when installed.
#[cfg(test)]
pub(crate) fn host_launcher() -> Option<std::path::PathBuf> {
    host::launcher()
}

use anyhow::{Context, Result};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use crate::fs::work_dir::WorkDir;
use crate::models::stage::CommandConfinement;
use crate::stage_cache::{DeveloperTools, StageCache};

use super::MemoryJournals;

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
    git: Option<GitDirs>,
    group_anchor: Option<super::spawn_observer::GroupAnchor>,
}

impl PreparedCommand {
    // This is the stable input environment. The wrapper maps its Cargo home and
    // temporary directories to private state under the versioned containment policy.
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

    /// The Git directories the command can read, whose state the pass cache
    /// fingerprints.
    pub(super) fn git_dirs(&self) -> Option<&GitDirs> {
        self.git.as_ref()
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
        let revision = format!("host-os-boundary-v9:{arguments:?}");
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
        let process = if self.group_anchor.is_some() {
            crate::process::OwnedProcess::child_only(child)
        } else {
            crate::process::OwnedProcess::new(child)
        };
        Ok(ConfinedChild {
            process,
            _group_anchor: self.group_anchor.take(),
            _scratch: self.scratch.take(),
        })
    }

    pub(super) fn has_group_anchor(&self) -> bool {
        self.group_anchor.is_some()
    }

    /// The pid of the group anchor the spawned command joined, if any.
    pub(super) fn anchor_pid(&self) -> Option<u32> {
        self.group_anchor.as_ref().map(|anchor| anchor.pgid())
    }

    pub(super) fn retire_group(&mut self) {
        if let Some(mut anchor) = self.group_anchor.take() {
            anchor.retire();
        }
    }

    pub(super) fn spawn_retaining_resources(&mut self) -> Result<Child> {
        if let Some(scratch) = &self.scratch {
            scratch.populate()?;
        }
        if let Some(wrapped) = self.wrapped.take() {
            self.command = wrapped;
        }
        self.group_anchor = super::spawn_observer::prepare_group()?;
        // A leader of its own session lets a timeout find descendants that
        // left its process group (`escapees`); under an anchor it joins the
        // anchor's group in the host job child's session instead. A new
        // session also has no controlling terminal: opening `/dev/tty` fails
        // with `ENXIO`, the leader's own `setpgid(0, 0)` fails with `EPERM`
        // (a session leader cannot leave its group), and on Linux the leader
        // acquires a controlling terminal by opening a terminal device
        // without `O_NOCTTY`.
        #[cfg(unix)]
        match &self.group_anchor {
            Some(anchor) => {
                self.command.process_group(i32::try_from(anchor.pgid())?);
            }
            None => super::escapees::lead_own_session(&mut self.command),
        }
        let child = self
            .command
            .spawn()
            .with_context(|| format!("Failed to spawn command: {}", self.display))?;
        Ok(child)
    }
}

/// An owned confined child: dropping it retires its process group before
/// removing its private tool state. Borrow the child for pipe and wait APIs.
pub struct ConfinedChild {
    process: crate::process::OwnedProcess,
    _group_anchor: Option<super::spawn_observer::GroupAnchor>,
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

/// Spawn a plan-authored command as a child process under `confinement`.
///
/// On Unix a normal child leads its own session and process group. Under the
/// host-job observer it joins a persistent guardian's group, which can still
/// be retired after the command exits or the daemon restarts.
///
/// stdin is `/dev/null` and both output streams are piped; callers must drain
/// them concurrently with waiting — `executor::run_spec_with_timeout` is the
/// one place that does so correctly.
pub fn spawn_confined(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    confinement: CommandConfinement,
) -> Result<ConfinedChild> {
    prepare_confined(spec, working_dir, None, confinement, None)?.spawn()
}

/// Resolve a confined command once, without spawning it yet. It starts in
/// `working_dir` and may read and write `tree`, the working tree containing
/// `working_dir`; with no `tree`, the working directory is the whole tree.
/// With `cache`, a confined command also writes the stage's build caches.
pub(super) fn prepare_confined(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    tree: Option<&Path>,
    confinement: CommandConfinement,
    cache: Option<&StageCache>,
) -> Result<PreparedCommand> {
    prepare_reading_journals(spec, working_dir, tree, confinement, cache, None)
}

/// [`prepare_confined`] for a command that may also read `journals`, the
/// plan's memory journals outside `tree`, read-only.
pub(super) fn prepare_reading_journals(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    tree: Option<&Path>,
    confinement: CommandConfinement,
    cache: Option<&StageCache>,
    journals: Option<&MemoryJournals>,
) -> Result<PreparedCommand> {
    let network = host::NetworkAccess::Denied;
    prepare(
        spec,
        working_dir,
        tree,
        confinement,
        network,
        cache,
        journals,
    )
}

/// Provision entries are approved package setup: retain the command boundary
/// while permitting the network needed to fetch dependencies.
pub(super) fn prepare_provision(
    spec: &CommandSpec,
    working_dir: &Path,
    cache: Option<&StageCache>,
) -> Result<PreparedCommand> {
    let (confined, network) = (CommandConfinement::Confined, host::NetworkAccess::Provision);
    prepare(
        spec,
        Some(working_dir),
        None,
        confined,
        network,
        cache,
        None,
    )
}

fn prepare(
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    tree: Option<&Path>,
    confinement: CommandConfinement,
    network: host::NetworkAccess,
    cache: Option<&StageCache>,
    journals: Option<&MemoryJournals>,
) -> Result<PreparedCommand> {
    let resolved_working_dir = working_dir
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok());
    let mut cmd = build_command(spec);
    match confinement {
        // Rebuild the child's environment from the documented host allowlist so
        // plan-authored commands never inherit loom's ambient credentials.
        CommandConfinement::Confined => apply_confined_environment(&mut cmd)?,
        // Explicit plan opt-in: the command genuinely needs what loom was given.
        CommandConfinement::Inherit => {}
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = &resolved_working_dir {
        cmd.current_dir(dir);
        if confinement == CommandConfinement::Confined {
            cmd.env("TMPDIR", dir).env("TMP", dir).env("TEMP", dir);
        }
    }
    let (wrapped, scratch, git) = if confinement == CommandConfinement::Confined {
        let (wrapped, scratch, git) = wrap(&cmd, tree, network, cache, journals)
            .with_context(|| format!("Failed to confine command: {spec}"))?;
        (Some(wrapped), Some(scratch), git)
    } else {
        (None, None, None)
    };
    Ok(PreparedCommand {
        command: cmd,
        working_dir: resolved_working_dir,
        display: spec.to_string(),
        spec: spec.clone(),
        wrapped,
        scratch,
        git,
        group_anchor: None,
    })
}

/// The running executable is a runtime dependency of `loom ...` criteria.
/// Derive it from this process, never from a caller-supplied `LOOM_BIN`.
/// The real macOS developer tools come next on `PATH`, before the program is
/// resolved, so a bare `git` or `cc` skips the xcrun shim, whose cache the
/// boundary denies. Every confined command gets them, a stage cache or not:
/// through the shim, each `cc` starts an `xcodebuild` lookup, and a cold
/// build with C dependencies outruns its timeout.
fn apply_confined_environment(command: &mut Command) -> Result<()> {
    crate::process::apply_stage_environment(command);
    let executable = std::env::current_exe().context("resolve the running Loom executable")?;
    let directory = executable
        .parent()
        .context("Loom executable has no parent directory")?;
    let host_path = command
        .get_envs()
        .find_map(|(key, value)| (key == "PATH").then_some(value).flatten())
        .map(OsStr::to_os_string);
    // `detect` looks the tools up once per process and caches the answer.
    let developer = DeveloperTools::detect()
        .map(|tools| tools.environment(host_path.as_ref()))
        .unwrap_or_default();
    let path = developer
        .get("PATH")
        .or(host_path.as_ref())
        .map_or(OsStr::new("/usr/bin:/bin"), OsString::as_os_str);
    let directories = std::iter::once(directory.to_path_buf())
        .chain(std::env::split_paths(path).filter(|entry| entry != directory));
    let path = std::env::join_paths(directories).context("build the confined command PATH")?;
    command.envs(&developer);
    command.env("PATH", path).env("LOOM_BIN", executable);
    Ok(())
}

/// The build cache of stage `stage_id` in the repository whose state
/// directory is `work_dir`, resolved through [`StageCache::resolve_from_process`].
pub(crate) fn stage_cache_for(work_dir: &Path, stage_id: &str) -> Result<StageCache> {
    let repo_root = WorkDir::new(work_dir)
        .ok()
        .and_then(|work_dir| work_dir.main_project_root())
        .with_context(|| format!("no repository root above {}", work_dir.display()))?;
    StageCache::resolve_from_process(&repo_root, stage_id)
}

/// [`stage_cache_for`] when `confinement` is `Confined`; an inherited run has
/// no boundary to grant the cache through, so it gets `None`.
pub(crate) fn confined_stage_cache(
    work_dir: &Path,
    stage_id: &str,
    confinement: CommandConfinement,
) -> Result<Option<StageCache>> {
    (confinement == CommandConfinement::Confined)
        .then(|| stage_cache_for(work_dir, stage_id))
        .transpose()
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
