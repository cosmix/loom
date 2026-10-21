//! OS boundary for host verification. Only the working tree, private scratch and
//! the command's Cargo home (see `confine_scratch.rs`) are writable, plus a
//! private /tmp and /var/tmp under bubblewrap; runtime and compiler
//! installation directories and a linked worktree's Git metadata
//! (`confine_git.rs`) are readable. Verification denies network access;
//! plan-approved provisioning can fetch dependencies.
//! A missing or unusable OS facility fails before executing repository code.
//! A command may start below its working tree's root (a package of a monorepo);
//! the whole tree stays visible so sibling packages and shared stores resolve.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(super) use super::scratch::CargoScratch;

#[path = "confine_git.rs"]
pub(super) mod git_roots;

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "confine_protected.rs"]
mod protected;

#[cfg(target_os = "macos")]
#[path = "confine_developer_tools.rs"]
mod developer_tools;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NetworkAccess {
    Denied,
    Provision,
}

/// Wrap `original` to start in its working directory with read-write access to
/// `tree`, the working tree containing it (the working directory when `None`).
pub(super) fn wrap(
    original: &Command,
    tree: Option<&Path>,
    network: NetworkAccess,
) -> Result<(Command, CargoScratch, Option<git_roots::GitDirs>)> {
    let cwd = original
        .get_current_dir()
        .context("confined command needs a working directory")?
        .canonicalize()
        .context("resolve confined working directory")?;
    let tree = confined_tree(tree, &cwd)?;
    let git = git_roots::git_dirs(&tree, network);
    let resolved_program = resolve_program(original, &cwd)?;
    // Only bubblewrap can bind a private directory at the host's Cargo home path.
    let mount = cfg!(target_os = "linux");
    let scratch = CargoScratch::prepare(original, &tree, mount)?;
    let mut command = platform_command(
        original,
        &tree,
        &cwd,
        &resolved_program,
        &scratch,
        network,
        git.as_ref(),
    )?;
    apply_environment(&mut command, original, &scratch);
    command
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    Ok((command, scratch, git))
}

/// `original`'s environment with private tool state from `scratch`. Git reads
/// no global config: the user's is unreadable inside the boundary, and Git
/// fails on a config file it cannot open (Seatbelt denies the read).
fn apply_environment(command: &mut Command, original: &Command, scratch: &CargoScratch) {
    command.env_clear();
    command.envs(original.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))));
    scratch.apply_environment(command);
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
/// must contain `cwd`, else `cwd` itself.
fn confined_tree(tree: Option<&Path>, cwd: &Path) -> Result<PathBuf> {
    let Some(tree) = tree else {
        return Ok(cwd.to_path_buf());
    };
    let tree = tree
        .canonicalize()
        .context("resolve confined working tree")?;
    anyhow::ensure!(
        cwd.starts_with(&tree),
        "confined working directory {} is outside its working tree {}",
        cwd.display(),
        tree.display()
    );
    Ok(tree)
}

fn environment_path(command: &Command, key: &str) -> Option<PathBuf> {
    command
        .get_envs()
        .find_map(|(name, value)| (name == key).then_some(value).flatten().map(PathBuf::from))
}

pub(super) fn cargo_home(command: &Command) -> Option<PathBuf> {
    environment_path(command, "CARGO_HOME")
        .or_else(|| environment_path(command, "HOME").map(|home| home.join(".cargo")))
}

fn read_roots(command: &Command) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = [
        "/usr",
        "/bin",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc",
        "/System",
        "/Library",
        "/dev",
        "/opt/homebrew",
        "/private/var/db/timezone",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = environment_path(command, "HOME") {
        roots.extend([
            home.join(".rustup"),
            home.join(".bun/bin"),
            home.join(".bun/install/cache"),
            home.join(".cache/uv"),
            home.join("Library/Caches/uv"),
            home.join(".local/share/uv/python"),
        ]);
    }
    if let Some(rustup) = environment_path(command, "RUSTUP_HOME") {
        roots.push(rustup);
    }
    if let Some(cargo) = cargo_home(command) {
        roots.extend([cargo.join("bin"), cargo.join("registry"), cargo.join("git")]);
    }
    // Expose only the running binary, not its installation directory (which
    // can contain unrelated user files). The preparer owns this environment entry.
    if let Some(executable) = environment_path(command, "LOOM_BIN") {
        roots.push(executable);
    }
    #[cfg(target_os = "macos")]
    roots.extend(developer_tools::developer_tools().iter().cloned());
    roots.into_iter().filter(|p| p.exists()).collect()
}

/// The working tree, the command's scratch, and its Cargo home when that is a
/// persistent per-tree directory.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn writable_roots<'a>(tree: &'a Path, scratch: &'a CargoScratch) -> Vec<&'a Path> {
    [tree, scratch.path()]
        .into_iter()
        .chain(scratch.linked_cargo_home())
        .collect()
}

/// Seatbelt read, write and deny rules for the tree, scratch, Cargo home, git
/// roots and the command's own read-only roots.
#[cfg(target_os = "macos")]
fn seatbelt_path_rules(
    original: &Command,
    tree: &Path,
    scratch: &CargoScratch,
    git: Option<&git_roots::GitDirs>,
) -> Result<String> {
    let quote = |path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap();
    let mut profile = String::new();
    let writable = writable_roots(tree, scratch);
    let readable = writable.iter().map(|path| path.to_path_buf());
    let readable = readable.chain(git.into_iter().flat_map(|git| git.roots.iter().cloned()));
    for root in read_roots(original).into_iter().chain(readable) {
        profile.push_str(&format!(
            "(allow file-read* (subpath {}))",
            quote(&root.canonicalize().unwrap_or(root))
        ));
    }
    let writable: Vec<_> = writable.into_iter().map(quote).collect();
    profile.push_str(&format!(
        "(allow file-write* (subpath {}))(allow file-write-data (literal \"/dev/null\"))",
        writable.join(")(subpath ")
    ));
    for root in read_roots(original)
        .into_iter()
        .filter(|p| p != Path::new("/dev"))
    {
        profile.push_str(&format!(
            "(deny file-write* (subpath {}))",
            quote(&root.canonicalize()?)
        ));
    }
    let paths = protected::protected(tree);
    profile.push_str(&protected::seatbelt_rules(&paths, quote));
    Ok(profile)
}

#[cfg(target_os = "macos")]
fn platform_command(
    original: &Command,
    tree: &Path,
    // Seatbelt keeps the process's own working directory, set by `wrap`.
    _cwd: &Path,
    program: &Path,
    scratch: &CargoScratch,
    network: NetworkAccess,
    git: Option<&git_roots::GitDirs>,
) -> Result<Command> {
    let launcher = Path::new("/usr/bin/sandbox-exec");
    if !launcher.is_file() {
        anyhow::bail!("confined verification requires sandbox-exec");
    }
    // dyld opens the root directory while discovering the system Cryptex cache.
    let mut profile = String::from("(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)(allow file-read* (literal \"/\"))");
    if network == NetworkAccess::Provision {
        profile.push_str("(allow network-outbound)");
    }
    profile.push_str(&seatbelt_path_rules(original, tree, scratch, git)?);
    let mut command = Command::new(launcher);
    command
        .arg("-p")
        .arg(profile)
        .arg(program)
        .args(original.get_args());
    Ok(command)
}

#[cfg(target_os = "linux")]
fn platform_command(
    original: &Command,
    tree: &Path,
    cwd: &Path,
    program: &Path,
    scratch: &CargoScratch,
    network: NetworkAccess,
    git: Option<&git_roots::GitDirs>,
) -> Result<Command> {
    let launcher =
        which::which("bwrap").context("confined verification requires bubblewrap (bwrap)")?;
    let mut command = Command::new(launcher);
    command.args([
        "--die-with-parent",
        "--unshare-all",
        "--new-session",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        // Private temporary directories, mounted before any bind so a tree
        // or scratch below them is mounted on top instead of hidden.
        "--tmpfs",
        "/tmp",
        "--tmpfs",
        "/var/tmp",
    ]);
    if network == NetworkAccess::Provision {
        command.arg("--share-net");
    }
    for writable in writable_roots(tree, scratch) {
        command.arg("--bind").arg(writable).arg(writable);
    }
    command.arg("--chdir").arg(cwd);
    bind_read_roots(&mut command, original, scratch);
    for root in git.into_iter().flat_map(|git| &git.roots) {
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

/// Read-only installation roots, and the private Cargo home when it is mounted.
/// Cargo's sources go read-only over that home, so they bind after it, and the
/// home binds after every other root in case one contains it.
#[cfg(target_os = "linux")]
fn bind_read_roots(command: &mut Command, original: &Command, scratch: &CargoScratch) {
    let mount = scratch.cargo_mount();
    let (cargo_roots, roots): (Vec<_>, Vec<_>) = read_roots(original)
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
    _: Option<&git_roots::GitDirs>,
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
