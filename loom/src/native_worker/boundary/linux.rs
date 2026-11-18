//! Bubblewrap argument list and environment of one boundary; `launch.rs`
//! spawns it.
//!
//! The order of the mount arguments is the policy: a later mount sits on top
//! of an earlier one. `--remount-ro /` is not recursive, so every bind keeps
//! the mode it was given.
use std::ffi::{OsStr, OsString};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::environment::{environment, lookup, scratch_probe, LOOM_BIN};
use super::mounts::{self, codex_package, CodexPackage};
use super::{tmpdir, BoundarySpec, SpawnedBoundary, BOUNDARY_TMPDIR};
use crate::verify::criteria::{
    bind_read_roots, bubblewrap, read_roots, CargoScratch, NetworkAccess,
};

/// Where the proxy socket directory is mounted inside the boundary: on the
/// read-only root, so no sandboxed command can rename it or an ancestor and
/// plant its own socket in its place.
const PROXY_DIR: &str = "/loom-proxy";
/// Size bounds of the private `/tmp` (holding `TMPDIR`, where compilers and
/// linkers write) and `/var/tmp` tmpfs, in bytes. Unbounded, each takes the
/// kernel default of half of RAM, so a few workers filling them could exhaust
/// host memory. Raise `/tmp` if a unit's build legitimately outgrows it.
pub(super) const TMPFS_BYTES: [u64; 2] = [4 << 30, 1 << 30];
/// Size bound of the private Codex home tmpfs, in bytes. It holds one worker's
/// sessions and rollouts; unbounded it takes the kernel default of half of RAM.
/// Needs `--size`, which the qualification probe requires (`probe.rs`
/// `tmpfs_size_gate`).
pub(super) const CODEX_HOME_TMPFS_BYTES: u64 = 1 << 30;

/// The real boundary's launcher and host proxy socket directory.
struct Launch {
    loom_program: PathBuf,
    proxy_dir: PathBuf,
}

/// One boundary to start: what it exposes and what it runs.
pub(super) struct Plan {
    /// Canonical worktree.
    pub(super) worktree: PathBuf,
    /// Private Codex home mounts (and the `W/.claude` bind) when set.
    pub(super) codex_home: Option<PathBuf>,
    package: Option<CodexPackage>,
    launch: Option<Launch>,
    pub(super) program: OsString,
    pub(super) args: Vec<OsString>,
}

impl Plan {
    /// `codex app-server` behind the in-boundary launcher.
    pub(super) fn real(spec: &BoundarySpec, host: &[(OsString, OsString)]) -> Result<Self> {
        let worktree = canonical_worktree(&spec.worktree)?;
        let package = codex_package(&spec.codex_program)?;
        let home = lookup(host, "HOME").map(PathBuf::from);
        let exposed = [Some(&worktree), Some(&spec.codex_home), home.as_ref()];
        mounts::refuse_wide_package(&package.root, exposed.into_iter().flatten())?;
        let proxy_dir = spec
            .proxy_socket
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .context("the proxy socket path has no parent directory")?
            .to_path_buf();
        let socket_name = spec
            .proxy_socket
            .file_name()
            .context("the proxy socket path has no file name")?;
        let mut args: Vec<OsString> = ["subagents", "worker-exec", "--proxy-socket"]
            .map(OsString::from)
            .into();
        args.extend([Path::new(PROXY_DIR).join(socket_name).into(), "--".into()]);
        args.push(package.program.clone().into());
        args.extend(["app-server", "--listen", "stdio://"].map(OsString::from));
        Ok(Self {
            worktree,
            codex_home: Some(spec.codex_home.clone()),
            package: Some(package),
            launch: Some(Launch {
                loom_program: spec.loom_program.clone(),
                proxy_dir,
            }),
            program: LOOM_BIN.into(),
            args,
        })
    }

    /// `program` directly, optionally with the private Codex home mounts.
    pub(super) fn payload(
        worktree: &Path,
        codex_home: Option<&Path>,
        program: &Path,
        args: &[OsString],
    ) -> Result<Self> {
        Ok(Self {
            worktree: canonical_worktree(worktree)?,
            codex_home: codex_home.map(Path::to_path_buf),
            package: None,
            launch: None,
            program: program.into(),
            args: args.to_vec(),
        })
    }

    /// The codex `package` run directly with `args`, its Codex home and package
    /// mounts and PATH as in the real boundary, without launcher or proxy: the
    /// qualification probe of Codex's own sandbox.
    pub(super) fn codex(
        worktree: &Path,
        codex_home: &Path,
        package: CodexPackage,
        args: &[OsString],
    ) -> Result<Self> {
        Ok(Self {
            worktree: canonical_worktree(worktree)?,
            codex_home: Some(codex_home.to_path_buf()),
            program: package.program.clone().into(),
            package: Some(package),
            launch: None,
            args: args.to_vec(),
        })
    }
}

fn canonical_worktree(worktree: &Path) -> Result<PathBuf> {
    let canonical = worktree
        .canonicalize()
        .with_context(|| format!("resolve the worktree {}", worktree.display()))?;
    anyhow::ensure!(
        canonical.is_dir(),
        "the worktree {} is not a directory",
        canonical.display()
    );
    Ok(canonical)
}

/// The bubblewrap command for `plan`, reporting its namespace on `info_fd`,
/// and the private Cargo scratch it mounts (which must outlive the boundary
/// and is removed with it). A plan exposing a path under [`BOUNDARY_TMPDIR`]
/// is refused.
pub(super) fn command(
    plan: &Plan,
    host: &[(OsString, OsString)],
    info_fd: i32,
) -> Result<(Command, CargoScratch)> {
    let codex_path = plan
        .package
        .as_ref()
        .and_then(|package| package.helper_dir.as_deref());
    refuse_exposed_tmpdir(plan, host)?;
    let env = environment(host, plan.launch.is_some(), codex_path);
    // `read_roots` and the Cargo scratch read the environment from a command.
    let mut probe = Command::new(&plan.program);
    probe.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    // Without `--size` support the native lane is not qualified (`probe.rs`).
    let mut command = bubblewrap(NetworkAccess::Denied, Some(TMPFS_BYTES))?;
    // The only writable temporary directory, on the private `/tmp` tmpfs.
    // `/tmp` itself stays outside the Codex turn policy's writable roots, so a
    // sandboxed command cannot rename anything bound below it.
    command.args(["--perms", "0700", "--dir", BOUNDARY_TMPDIR]);
    command.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    let scratch_env = scratch_probe(&plan.program, &env, host);
    // Private to this boundary: its commands may write the Cargo home.
    let scratch = CargoScratch::prepare_private(&scratch_env, &plan.worktree)?;
    command.arg("--info-fd").arg(info_fd.to_string());
    bind_read_roots(&mut command, &probe, &scratch);
    if let Some(home) = scratch.linked_cargo_home() {
        command.arg("--bind").arg(home).arg(home);
        command.env("CARGO_HOME", home);
    }
    mounts::worktree(&mut command, &plan.worktree, plan.codex_home.is_some())?;
    if let Some(codex_home) = &plan.codex_home {
        let roots = read_roots(&probe);
        let package_root = plan
            .package
            .as_ref()
            .filter(|package| !roots.iter().any(|root| package.program.starts_with(root)))
            .map(|package| package.root.as_path());
        mounts::codex_home(&mut command, codex_home, package_root)?;
    }
    if let Some(launch) = &plan.launch {
        bind_launch(&mut command, &launch.loom_program, &launch.proxy_dir);
    }
    command.arg("--chdir").arg(&plan.worktree);
    command.args(["--remount-ro", "/", "--"]);
    command.arg(&plan.program).args(&plan.args);
    tmpdir::refuse_mount_targets(&command)?;
    Ok((command, scratch))
}

/// Refuse a plan whose worktree, Codex home or host `HOME`, `TMPDIR`,
/// `CARGO_HOME` or `RUSTUP_HOME` lies under [`BOUNDARY_TMPDIR`].
fn refuse_exposed_tmpdir(plan: &Plan, host: &[(OsString, OsString)]) -> Result<()> {
    let planned = [("the worktree", plan.worktree.as_path())]
        .into_iter()
        .chain(
            plan.codex_home
                .as_deref()
                .map(|home| ("the Codex home", home)),
        );
    let named = ["HOME", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME"]
        .into_iter()
        .filter_map(|key| Some((key, Path::new(lookup(host, key)?))));
    tmpdir::refuse_exposed(planned.chain(named))
}

/// The launcher and the proxy socket directory, read-only at fixed paths on the
/// read-only root, so no sandboxed command can rename or replace them.
fn bind_launch(
    command: &mut Command,
    loom_program: impl AsRef<OsStr>,
    proxy_dir: impl AsRef<OsStr>,
) {
    command.arg("--ro-bind").arg(loom_program).arg(LOOM_BIN);
    command.arg("--ro-bind").arg(proxy_dir).arg(PROXY_DIR);
}

/// Start `plan` from the calling thread (`--die-with-parent` follows the
/// spawning thread). An `Err` that leaves no namespace is a plain error; one
/// after bubblewrap may have created it is a
/// [`StartedThenFailed`](super::launch::StartedThenFailed).
pub(super) fn start(plan: &Plan, host: &[(OsString, OsString)]) -> Result<SpawnedBoundary> {
    let (reader, writer) = std::io::pipe().context("create the boundary info pipe")?;
    let (command, scratch) = command(plan, host, writer.as_raw_fd())?;
    super::launch::launch(command, scratch, reader, writer)
}
