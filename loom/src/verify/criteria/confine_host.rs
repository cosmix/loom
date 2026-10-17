//! OS boundary for host verification. Only the working tree and private scratch are writable;
//! runtime and compiler installation directories are readable. Verification denies
//! network access; plan-approved provisioning can fetch dependencies.
//! A missing or unusable OS facility fails before executing repository code.
//! A command may start below its working tree's root (a package of a monorepo);
//! the whole tree stays visible so sibling packages and shared stores resolve.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
) -> Result<(Command, CargoScratch)> {
    let cwd = original
        .get_current_dir()
        .context("confined command needs a working directory")?
        .canonicalize()
        .context("resolve confined working directory")?;
    let tree = confined_tree(tree, &cwd)?;
    let resolved_program = resolve_program(original, &cwd)?;
    let scratch = CargoScratch::prepare(original, &tree)?;
    // The outer boundary is reused only when it covers the whole granted tree.
    let mut command =
        if network == NetworkAccess::Denied && crate::daemon::inherited::available(&tree) {
            let mut command = Command::new(&resolved_program);
            command.args(original.get_args());
            command
        } else {
            platform_command(
                original,
                &tree,
                &cwd,
                &resolved_program,
                scratch.path(),
                network,
            )?
        };
    command.env_clear();
    command.envs(original.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))));
    scratch.apply_environment(&mut command);
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
    Ok((command, scratch))
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

fn cargo_home(command: &Command) -> Option<PathBuf> {
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
    roots.extend(developer_tools().iter().cloned());
    roots.into_iter().filter(|p| p.exists()).collect()
}

#[cfg(target_os = "macos")]
fn developer_tools() -> &'static Option<PathBuf> {
    static ROOT: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let mut command = Command::new("/usr/bin/xcode-select");
        command.env_clear().arg("--print-path");
        let output = crate::process::run_bounded(&mut command, std::time::Duration::from_secs(2))
            .ok()?
            .completed()?;
        if !output.status.success() {
            return None;
        }
        let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
        let path = path
            .is_absolute()
            .then(|| path.canonicalize().ok())
            .flatten()?;
        // Xcode's selected Developer directory links frameworks in sibling Contents paths.
        Some(
            if path.file_name().is_some_and(|name| name == "Developer")
                && path
                    .parent()
                    .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "Contents"))
            {
                path.parent()?.to_path_buf()
            } else {
                path
            },
        )
    })
}

/// Keep tool state outside the input tree: recursive linters follow the Cargo
/// source symlinks, and Biome's Git ignore matcher panics on their external paths.
/// Populate only at spawn; no repository inputs or host credentials are copied.
pub(super) struct CargoScratch {
    directory: tempfile::TempDir,
    sources: Vec<(String, PathBuf)>,
}

impl CargoScratch {
    fn prepare(command: &Command, tree: &Path) -> Result<Self> {
        let directory = private_temporary_directory(tree)?;
        let mut sources = Vec::new();
        if let Some(cargo) = cargo_home(command) {
            for name in ["registry", "git"] {
                let source = cargo.join(name);
                if source.is_dir() {
                    sources.push((name.to_string(), source.canonicalize()?));
                }
            }
        }
        Ok(Self { directory, sources })
    }

    pub(super) fn path(&self) -> &Path {
        self.directory.path()
    }

    fn apply_environment(&self, command: &mut Command) {
        for key in ["CARGO_HOME", "TMPDIR", "TMP", "TEMP"] {
            command.env(key, self.path());
        }
        for (key, child) in [
            ("XDG_CACHE_HOME", "cache"),
            ("UV_CACHE_DIR", "uv-cache"),
            ("BUN_INSTALL_CACHE_DIR", "bun-cache"),
        ] {
            command.env(key, self.path().join(child));
        }
    }

    pub(super) fn populate(&self) -> Result<()> {
        #[cfg(unix)]
        for (name, source) in &self.sources {
            std::os::unix::fs::symlink(source, self.directory.path().join(name))?;
        }
        Ok(())
    }
}

fn private_temporary_directory(tree: &Path) -> Result<tempfile::TempDir> {
    let mut failure = None;
    // Keep the default first for inherited macOS temporary-directory grants.
    // /var/tmp also supports callers whose working tree is /tmp itself.
    for root in [std::env::temp_dir(), "/tmp".into(), "/var/tmp".into()] {
        let root = match root.canonicalize() {
            Ok(root) if !root.starts_with(tree) => root,
            _ => continue,
        };
        match tempfile::Builder::new()
            .prefix(".loom-command-")
            .tempdir_in(&root)
        {
            Ok(directory) => return Ok(directory),
            Err(error) => failure = Some(error),
        }
    }
    match failure {
        Some(error) => {
            Err(error).context("create private command scratch outside its working tree")
        }
        None => anyhow::bail!("command scratch must be outside its working tree"),
    }
}

#[cfg(target_os = "macos")]
fn platform_command(
    original: &Command,
    tree: &Path,
    // Seatbelt keeps the process's own working directory, set by `wrap`.
    _cwd: &Path,
    program: &Path,
    scratch: &Path,
    network: NetworkAccess,
) -> Result<Command> {
    let launcher = Path::new("/usr/bin/sandbox-exec");
    if !launcher.is_file() {
        anyhow::bail!("confined verification requires sandbox-exec");
    }
    let quote = |path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap();
    // dyld opens the root directory while discovering the system Cryptex cache.
    let mut profile = String::from("(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)(allow file-read* (literal \"/\"))");
    if network == NetworkAccess::Provision {
        profile.push_str("(allow network-outbound)");
    }
    for root in read_roots(original)
        .into_iter()
        .chain([tree.to_path_buf(), scratch.to_path_buf()])
    {
        profile.push_str(&format!(
            "(allow file-read* (subpath {}))",
            quote(&root.canonicalize().unwrap_or(root))
        ));
    }
    profile.push_str(&format!(
        "(allow file-write* (subpath {})(subpath {}))(allow file-write-data (literal \"/dev/null\"))",
        quote(tree), quote(scratch)
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
    scratch: &Path,
    network: NetworkAccess,
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
    ]);
    if network == NetworkAccess::Provision {
        command.arg("--share-net");
    }
    command
        .arg("--bind")
        .arg(tree)
        .arg(tree)
        .arg("--bind")
        .arg(scratch)
        .arg(scratch)
        .arg("--chdir")
        .arg(cwd);
    for root in read_roots(original)
        .into_iter()
        .filter(|p| p != Path::new("/dev"))
    {
        command.arg("--ro-bind").arg(&root).arg(&root);
    }
    if network == NetworkAccess::Provision {
        bind_resolver_target(&mut command, Path::new("/etc/resolv.conf"))?;
    }
    // The new root is a writable tmpfs in which `--bind` created the working
    // tree's parents; left writable, writes outside the working tree would
    // succeed privately instead of failing as the Seatbelt profile makes them.
    // The remount is not recursive: the binds above keep their own modes.
    command.args(["--remount-ro", "/"]);
    command.arg("--").arg(program).args(original.get_args());
    Ok(command)
}

/// /etc can contain a resolver symlink into /run, which stays hidden. Expose
/// only its target file for provisioning, without granting access to host sockets.
#[cfg(target_os = "linux")]
fn bind_resolver_target(command: &mut Command, resolver: &Path) -> Result<()> {
    if resolver.is_symlink() {
        let target = resolver
            .canonicalize()
            .context("resolve provisioning DNS configuration")?;
        command.arg("--ro-bind").arg(&target).arg(&target);
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_command(
    _: &Command,
    _: &Path,
    _: &Path,
    _: &Path,
    _: &Path,
    _: NetworkAccess,
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
