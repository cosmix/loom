//! OS boundary for host verification. Only the working tree, private scratch and
//! the command's Cargo home (see `confine_scratch.rs`) are writable, plus a
//! private /tmp and /var/tmp under bubblewrap; runtime and compiler
//! installation directories and a linked worktree's Git metadata
//! (`confine_git.rs`) are readable. Verification denies network access;
//! plan-approved provisioning can fetch dependencies. Warm uv and Bun caches are
//! read-only lowers beneath each command's private cache (`confine_scratch.rs`).
//! A missing or unusable OS facility fails before executing repository code.
//! A command may start below its working tree's root (a package of a monorepo);
//! the whole tree stays visible so sibling packages and shared stores resolve.
//! A stage's cache root (`crate::stage_cache`) is writable too; its variables
//! replace the per-command caches and their overlays. Host build-cache seeds and
//! toolchains installed under the home are readable for every command.
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::stage_cache::{go_proxy, host_env_from_process, Consumer, StageCache};

pub(super) use super::scratch::CargoScratch;

#[path = "confine_git.rs"]
pub(super) mod git_roots;

#[path = "confine_read_roots.rs"]
mod roots;
#[cfg(any(target_os = "linux", test))]
pub(crate) use roots::read_roots;
use roots::read_roots_beside;
pub(crate) use roots::{cargo_home, environment_path};

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "confine_protected.rs"]
mod protected;

#[cfg(target_os = "macos")]
#[path = "confine_developer_tools.rs"]
mod developer_tools;

#[cfg(any(target_os = "macos", test))]
#[path = "confine_ancestors.rs"]
mod ancestors;

#[cfg(target_os = "macos")]
#[path = "confine_seatbelt.rs"]
mod seatbelt;
#[cfg(target_os = "macos")]
use seatbelt::platform_command;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetworkAccess {
    Denied,
    Provision,
}

/// What a boundary grants beyond the tree and the command's scratch.
#[derive(Default)]
pub(super) struct Grants<'a> {
    /// A linked worktree's Git metadata, read-only.
    pub(super) git: Option<&'a git_roots::GitDirs>,
    /// The canonical stage cache root: writable, and on macOS the only place
    /// the host-job sccache socket may be bound and reached.
    pub(super) cache_root: Option<&'a Path>,
}

impl Grants<'_> {
    /// The environment the stage cache's seeds are named from, when there is a
    /// stage cache: loom's own, so a seed's read root and the variable naming
    /// it agree.
    pub(super) fn seed_env(&self) -> Option<BTreeMap<String, OsString>> {
        self.cache_root.map(|_| host_env_from_process())
    }
}

/// Wrap `original` to start in its working directory with read-write access to
/// `tree`, the working tree containing it (the working directory when `None`),
/// and to `cache`, the stage's build-cache root, when given.
pub(super) fn wrap(
    original: &Command,
    tree: Option<&Path>,
    network: NetworkAccess,
    cache: Option<&StageCache>,
) -> Result<(Command, CargoScratch, Option<git_roots::GitDirs>)> {
    let cwd = original
        .get_current_dir()
        .context("confined command needs a working directory")?
        .canonicalize()
        .context("resolve confined working directory")?;
    let tree = confined_tree(tree, &cwd)?;
    let git = git_roots::git_dirs(&tree, network);
    let resolved_program = resolve_program(original, &cwd)?;
    let cache_root = cache
        .map(|cache| cache.ensure().context("prepare the stage build cache"))
        .transpose()?;
    // Grant, bind and variables name the one canonical root: the configured
    // one can reach it through a symlink the boundary would not recreate.
    let cache = cache
        .zip(cache_root.as_ref())
        .map(|(cache, root)| cache.clone().at_root(root.clone()));
    let cache = cache.as_ref();
    // Only bubblewrap can bind a private directory at the host's Cargo home path.
    let mount = cfg!(target_os = "linux");
    let scratch = CargoScratch::prepare(original, &tree, mount, cache.is_some())?;
    let grants = Grants {
        git: git.as_ref(),
        cache_root: cache_root.as_deref(),
    };
    let mut command = platform_command(
        original,
        &tree,
        &cwd,
        &resolved_program,
        &scratch,
        network,
        &grants,
    )?;
    apply_environment(&mut command, original, &scratch, cache, &tree);
    if cache.is_some() && network == NetworkAccess::Provision {
        // Provisioning fetches, so Go takes the session's proxy chain
        // after the seed instead of a host job's `off`.
        let proxy = go_proxy(Consumer::Session, &host_env_from_process());
        command.env("GOPROXY", proxy);
    }
    prepare_spawn(&mut command, &cwd);
    Ok((command, scratch, git))
}

/// Start in `cwd` with no input and piped output. The spawn chooses the
/// command's session and process group (`PreparedCommand::spawn_retaining_resources`).
fn prepare_spawn(command: &mut Command, cwd: &Path) {
    command
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
}

/// `original`'s environment with private tool state from `scratch`. Git reads
/// no global config: the user's is unreadable inside the boundary, and Git
/// fails on a config file it cannot open (Seatbelt denies the read). A stage
/// cache replaces the scratch's package-manager caches: its variables come
/// after the scratch's so the stage's caches win. `PATH` and the
/// developer-tools variables are already on `original`
/// (`confine.rs::apply_confined_environment`), where the program is resolved;
/// seeds resolve from loom's own environment.
fn apply_environment(
    command: &mut Command,
    original: &Command,
    scratch: &CargoScratch,
    cache: Option<&StageCache>,
    tree: &Path,
) {
    command.env_clear();
    command.envs(original.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))));
    scratch.apply_environment(command);
    if let Some(cache) = cache {
        let host = host_env_from_process();
        command.envs(cache.environment(Consumer::HostJob, tree, &host, None));
    }
    command.env("GIT_CONFIG_GLOBAL", "/dev/null");
    if cfg!(target_os = "linux") {
        // Bubblewrap mounts a private /tmp, so temporary files need no scratch
        // path; inside a session that path lies under the harness's own
        // /tmp/claude-<uid>, which tests and hooks treat specially.
        for key in ["TMPDIR", "TMP", "TEMP"] {
            command.env(key, "/tmp");
        }
    }
}

fn resolve_program(original: &Command, cwd: &Path) -> Result<PathBuf> {
    let program = original.get_program();
    let path = original
        .get_envs()
        .find_map(|(k, v)| (k == "PATH").then_some(v).flatten());
    which::which_in(program, path, cwd)
        .with_context(|| format!("Failed to resolve confined command: {:?}", program))
}

/// The tree a command starting in the canonical `cwd` may use: `tree`, which
/// must contain `cwd`, else `cwd` itself. The tree is writable to the code it
/// holds, which could swap it for a symlink after a full resolve; only its
/// parent is resolved, and the tree itself must be a directory, so the
/// boundary grants that path and never a link's target.
fn confined_tree(tree: Option<&Path>, cwd: &Path) -> Result<PathBuf> {
    let Some(tree) = tree else {
        return Ok(cwd.to_path_buf());
    };
    let tree = match (tree.parent(), tree.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
            parent.canonicalize().map(|parent| parent.join(name))
        }
        _ => tree.canonicalize(),
    }
    .context("resolve confined working tree")?;
    let metadata = tree
        .symlink_metadata()
        .context("resolve confined working tree")?;
    anyhow::ensure!(
        metadata.is_dir(),
        "confined working tree {} must be a directory, not a symlink",
        tree.display()
    );
    anyhow::ensure!(
        cwd.starts_with(&tree),
        "confined working directory {} is outside its working tree {}",
        cwd.display(),
        tree.display()
    );
    Ok(tree)
}

/// The working tree, the command's scratch, and its Cargo home when that is a
/// persistent per-tree directory, and the stage cache root when granted.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn writable_roots<'a>(
    tree: &'a Path,
    scratch: &'a CargoScratch,
    grants: &Grants<'a>,
) -> Vec<&'a Path> {
    [tree, scratch.path()]
        .into_iter()
        .chain(scratch.linked_cargo_home())
        .chain(grants.cache_root)
        .collect()
}

#[cfg(target_os = "linux")]
fn platform_command(
    original: &Command,
    tree: &Path,
    cwd: &Path,
    program: &Path,
    scratch: &CargoScratch,
    network: NetworkAccess,
    grants: &Grants<'_>,
) -> Result<Command> {
    let mut command = bubblewrap(network, None)?;
    let writable = writable_roots(tree, scratch, grants);
    for root in &writable {
        command.arg("--bind").arg(root).arg(root);
    }
    command.arg("--chdir").arg(cwd);
    bind_roots(
        &mut command,
        read_roots_beside(original, &writable, grants.seed_env().as_ref()),
        scratch,
    );
    for (source, upper, work, target) in scratch.cache_overlays() {
        command.arg("--overlay-src").arg(source);
        command.arg("--overlay").arg(upper).arg(work).arg(target);
    }
    for root in grants.git.into_iter().flat_map(|git| &git.roots) {
        command.arg("--ro-bind").arg(root).arg(root);
    }
    protected::bind(&mut command, &protected::protected(tree));
    bind_resolver_target(&mut command, Path::new("/etc/resolv.conf"))?;
    // The new root is a writable tmpfs in which `--bind` created the working
    // tree's parents; left writable, writes outside the working tree and the
    // private temporary directories would succeed privately instead of failing
    // as the Seatbelt profile makes them.
    // The remount is not recursive: the binds above keep their own modes.
    command.args(["--remount-ro", "/"]);
    command.arg("--").arg(program).args(original.get_args());
    Ok(command)
}

/// The launcher with its namespaces and private system mounts, before any bind.
/// `tmpfs_bytes` bounds `/tmp` and `/var/tmp` (bubblewrap 0.10 or later);
/// `None` leaves each the kernel default, half of RAM.
#[cfg(target_os = "linux")]
pub(crate) fn bubblewrap(network: NetworkAccess, tmpfs_bytes: Option<[u64; 2]>) -> Result<Command> {
    let launcher =
        which::which("bwrap").context("confined verification requires bubblewrap (bwrap)")?;
    let mut command = Command::new(launcher);
    command.args(["--die-with-parent", "--unshare-all", "--new-session"]);
    command.args(["--proc", "/proc", "--dev", "/dev"]);
    // Private temporary directories, mounted before any bind so a tree or
    // scratch below them is mounted on top instead of hidden.
    for (index, dir) in ["/tmp", "/var/tmp"].into_iter().enumerate() {
        if let Some(bytes) = tmpfs_bytes {
            command.arg("--size").arg(bytes[index].to_string());
        }
        command.args(["--tmpfs", dir]);
    }
    if network == NetworkAccess::Provision {
        command.arg("--share-net");
    }
    Ok(command)
}

/// Read-only installation roots, and the private Cargo home when it is mounted.
/// Cargo's sources go read-only over that home, so they bind after it, and the
/// home binds after every other root in case one contains it.
#[cfg(target_os = "linux")]
pub(crate) fn bind_read_roots(command: &mut Command, original: &Command, scratch: &CargoScratch) {
    bind_roots(command, read_roots(original), scratch);
}

/// [`bind_read_roots`] over `read_roots`, already chosen.
#[cfg(target_os = "linux")]
fn bind_roots(command: &mut Command, read_roots: Vec<PathBuf>, scratch: &CargoScratch) {
    let mount = scratch.cargo_mount();
    let (cargo_roots, roots): (Vec<_>, Vec<_>) = read_roots
        .into_iter()
        .filter(|p| p != Path::new("/dev"))
        .partition(|root| {
            mount
                .as_ref()
                .is_some_and(|(_, home)| root.starts_with(home))
        });
    for root in roots {
        command.arg("--ro-bind").arg(&root).arg(&root);
    }
    if let Some((private, home)) = &mount {
        command.arg("--bind").arg(private).arg(home);
    }
    for root in cargo_roots {
        command.arg("--ro-bind").arg(&root).arg(&root);
    }
}

/// /etc can contain a resolver symlink into /run, which stays hidden. Expose
/// only its target file, read-only, so the link resolves as it does outside,
/// without granting access to host sockets. A target that is absent (hidden by
/// an enclosing sandbox) leaves the link dangling there too, so nothing binds.
#[cfg(target_os = "linux")]
fn bind_resolver_target(command: &mut Command, resolver: &Path) -> Result<()> {
    if !resolver.is_symlink() {
        return Ok(());
    }
    match resolver.canonicalize() {
        Ok(target) => {
            command.arg("--ro-bind").arg(&target).arg(&target);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("resolve the DNS configuration"),
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_command(
    _: &Command,
    _: &Path,
    _: &Path,
    _: &Path,
    _: &CargoScratch,
    _: NetworkAccess,
    _: &Grants<'_>,
) -> Result<Command> {
    anyhow::bail!("confined verification is unsupported on this platform")
}

#[cfg(test)]
pub(super) fn launcher() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return Path::new("/usr/bin/sandbox-exec")
        .is_file()
        .then(|| PathBuf::from("/usr/bin/sandbox-exec"));
    #[cfg(target_os = "linux")]
    return which::which("bwrap").ok();
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

#[cfg(test)]
#[path = "confine_host_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "confine_host_tool_tests.rs"]
mod tool_tests;

#[cfg(test)]
#[path = "confine_cargo_home_tests.rs"]
mod cargo_home_tests;

#[cfg(test)]
#[path = "confine_stage_cache_tests.rs"]
mod stage_cache_tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "confine_scratch/warm_cache_tests.rs"]
mod warm_cache_tests;
