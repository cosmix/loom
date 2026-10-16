//! OS boundary for host verification. Only the working tree is writable;
//! runtime and compiler installation directories are readable, network is denied.
//! A missing or unusable OS facility fails before executing repository code.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(super) fn wrap(original: &Command) -> Result<(Command, CargoScratch)> {
    let cwd = original
        .get_current_dir()
        .context("confined command needs a working directory")?
        .canonicalize()
        .context("resolve confined working directory")?;
    let program = original.get_program();
    let resolved_program = which::which_in(
        program,
        original
            .get_envs()
            .find_map(|(k, v)| (k == "PATH").then_some(v).flatten()),
        &cwd,
    )
    .with_context(|| format!("Failed to resolve confined command: {:?}", program))?;
    let mut command = if crate::daemon::inherited::available(&cwd) {
        let mut command = Command::new(&resolved_program);
        command.args(original.get_args());
        command
    } else {
        platform_command(original, &cwd, &resolved_program)?
    };
    let scratch = CargoScratch::prepare(original, &cwd)?;
    command.env_clear();
    command.envs(original.get_envs().filter_map(|(k, v)| v.map(|v| (k, v))));
    command.env("CARGO_HOME", scratch.directory.path());
    command.env("TMPDIR", scratch.directory.path());
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
        roots.extend([home.join(".rustup"), home.join(".bun/bin")]);
    }
    if let Some(rustup) = environment_path(command, "RUSTUP_HOME") {
        roots.push(rustup);
    }
    if let Some(cargo) = cargo_home(command) {
        roots.extend([cargo.join("bin"), cargo.join("registry"), cargo.join("git")]);
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

/// Freeze source paths at preparation, but leave the directory empty until spawn
/// so cache fingerprinting does not mistake runtime artifacts for repository input.
pub(super) struct CargoScratch {
    directory: tempfile::TempDir,
    sources: Vec<(String, PathBuf)>,
}

impl CargoScratch {
    fn prepare(command: &Command, cwd: &Path) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix(".loom-cargo-")
            .tempdir_in(cwd)?;
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

    pub(super) fn populate(&self) -> Result<()> {
        #[cfg(unix)]
        for (name, source) in &self.sources {
            std::os::unix::fs::symlink(source, self.directory.path().join(name))?;
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn platform_command(original: &Command, cwd: &Path, program: &Path) -> Result<Command> {
    let launcher = Path::new("/usr/bin/sandbox-exec");
    if !launcher.is_file() {
        anyhow::bail!("confined verification requires sandbox-exec");
    }
    let quote = |path: &Path| serde_json::to_string(&path.to_string_lossy()).unwrap();
    // dyld opens the root directory while discovering the system Cryptex cache.
    let mut profile = String::from("(version 1)(deny default)(allow process*)(allow sysctl-read)(allow mach-lookup)(allow file-read-metadata)(allow file-read* (literal \"/\"))");
    for root in read_roots(original)
        .into_iter()
        .chain(std::iter::once(cwd.to_path_buf()))
    {
        profile.push_str(&format!(
            "(allow file-read* (subpath {}))",
            quote(&root.canonicalize().unwrap_or(root))
        ));
    }
    profile.push_str(&format!(
        "(allow file-write* (subpath {}))(allow file-write-data (literal \"/dev/null\"))",
        quote(cwd)
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
fn platform_command(original: &Command, cwd: &Path, program: &Path) -> Result<Command> {
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
    command
        .arg("--bind")
        .arg(cwd)
        .arg(cwd)
        .arg("--chdir")
        .arg(cwd);
    for root in read_roots(original)
        .into_iter()
        .filter(|p| p != Path::new("/dev"))
    {
        command.arg("--ro-bind").arg(&root).arg(&root);
    }
    command.arg("--").arg(program).args(original.get_args());
    Ok(command)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn platform_command(_: &Command, _: &Path, _: &Path) -> Result<Command> {
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
