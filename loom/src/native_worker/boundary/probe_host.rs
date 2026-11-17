//! The real host's qualification probes: the host bubblewrap, a payload
//! boundary, and Codex's own sandbox run inside one.
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use super::super::environment::{environment, host_cargo_home};
use super::super::linux::{self, Plan};
use super::super::mounts::{codex_package, CodexPackage};
use super::super::quiescence::kill_namespace;
use super::super::{keep_scratch_unless_released, SpawnedBoundary};
use super::{HostProbes, Nesting};
use crate::verify::criteria::read_roots;

/// How long one probe boundary may run.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// `codex sandbox` as a turn runs commands: workspace-write with no network
/// and `/tmp` excluded (`protocol/wire.rs::turn_start`).
pub(super) const SANDBOX_ARGS: [&str; 9] = [
    "sandbox",
    "-c",
    "sandbox_mode=\"workspace-write\"",
    "-c",
    "sandbox_workspace_write.network_access=false",
    "-c",
    "sandbox_workspace_write.exclude_slash_tmp=true",
    "--",
    "true",
];

/// Where a standalone codex release keeps the bubblewrap it falls back to.
const BUNDLED_BWRAP: &str = "codex-resources/bwrap";

pub(super) struct RealHost;

impl HostProbes for RealHost {
    fn bwrap_help(&self) -> Result<String, String> {
        let bwrap =
            which::which("bwrap").map_err(|_| "bubblewrap (bwrap) is not installed".to_string())?;
        let help = Command::new(&bwrap)
            .arg("--help")
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("cannot run bwrap --help: {error}"))?;
        Ok(String::from_utf8_lossy(&help.stdout).into_owned())
    }

    fn boundary(&self) -> Result<(ExitStatus, u64), String> {
        let worktree = tempfile::tempdir().map_err(|error| format!("probe worktree: {error}"))?;
        let plan = Plan::payload(worktree.path(), None, Path::new("true"), &[])
            .map_err(|error| format!("{error:#}"))?;
        let host: Vec<_> = std::env::vars_os().collect();
        run(&plan, &host).map(|(status, namespace, _)| (status, namespace))
    }

    fn own_pid_namespace(&self) -> Result<PathBuf, String> {
        std::fs::read_link("/proc/self/ns/pid")
            .map_err(|error| format!("cannot read this process's pid namespace: {error}"))
    }

    fn nesting(&self) -> Result<Nesting, String> {
        let program = crate::codex::find_codex_path()
            .and_then(|path| Ok(path.canonicalize()?))
            .map_err(|error| format!("codex executable not found: {error:#}"))?;
        let package = codex_package(&program).map_err(|error| format!("{error:#}"))?;
        let dir = tempfile::tempdir().map_err(|error| format!("probe directory: {error}"))?;
        let root = dir
            .path()
            .canonicalize()
            .map_err(|e| format!("probe directory: {e}"))?;
        let (w, home) = (root.join("w"), root.join("home"));
        let codex_home = home.join(".codex");
        for dir in [w.join(".claude"), codex_home.join("sessions")] {
            std::fs::create_dir_all(&dir).map_err(|error| format!("probe directory: {error}"))?;
        }
        let host = probe_host(&home);
        let bwrap = visible_codex_bwrap(&host, &package);
        let args = SANDBOX_ARGS.map(OsString::from);
        let plan =
            Plan::codex(&w, &codex_home, package, &args).map_err(|error| format!("{error:#}"))?;
        let (status, _, stderr) = run(&plan, &host)?;
        Ok(Nesting {
            ran: status.success(),
            bwrap,
            stderr,
        })
    }
}

/// The host environment with `HOME` at the probe's own `home` (its Codex home
/// is `home/.codex`) and the host Cargo home named, so the probe's PATH and
/// read-only roots are the real boundary's.
fn probe_host(home: &Path) -> Vec<(OsString, OsString)> {
    let mut host: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let cargo = host_cargo_home(&host);
    host.retain(|(key, _)| key != "HOME" && key != "CARGO_HOME");
    host.push(("HOME".into(), home.into()));
    host.extend(cargo.map(|cargo| ("CARGO_HOME".into(), cargo.into())));
    host
}

/// The bubblewrap Codex's sandbox runs inside the real boundary for `host`.
fn visible_codex_bwrap(host: &[(OsString, OsString)], package: &CodexPackage) -> Option<PathBuf> {
    let env = environment(host, true, package.helper_dir.as_deref());
    let mut probe = Command::new(&package.program);
    probe.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    let mut visible = read_roots(&probe);
    visible.push(package.root.clone());
    let path = env.iter().find(|(key, _)| key == "PATH")?;
    codex_bwrap(&path.1, &visible, &package.root)
}

/// The first executable `bwrap` on `path` under a `visible` root, else a
/// standalone release's bundled one under `package_root`: codex 0.169 looks
/// on PATH first, then next to its executable.
pub(super) fn codex_bwrap(
    path: &OsStr,
    visible: &[PathBuf],
    package_root: &Path,
) -> Option<PathBuf> {
    let executable = |file: &Path| {
        file.metadata()
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    };
    std::env::split_paths(path)
        .map(|dir| dir.join("bwrap"))
        .find(|file| executable(file) && visible.iter().any(|root| file.starts_with(root)))
        .or_else(|| {
            let bundled = package_root.join(BUNDLED_BWRAP);
            (package_root.is_dir() && executable(&bundled)).then_some(bundled)
        })
}

/// Run `plan` for at most [`PROBE_TIMEOUT`]; always retired. The exit
/// status, the namespace inode and the start of stderr.
fn run(plan: &Plan, host: &[(OsString, OsString)]) -> Result<(ExitStatus, u64, String), String> {
    let boundary = linux::start(plan, host).map_err(|error| format!("{error:#}"))?;
    finish_within(boundary, PROBE_TIMEOUT)
}

/// Wait at most `timeout` for `boundary` to exit, then [`retire`] it.
fn finish_within(
    mut boundary: SpawnedBoundary,
    timeout: Duration,
) -> Result<(ExitStatus, u64, String), String> {
    let waited = boundary.process.wait(timeout);
    retire(boundary, waited, timeout)
}

/// Retire `boundary`, which `waited` for at most `timeout`. Its namespace is
/// killed and emptied before its scratch is dropped, even when the outer
/// bubblewrap exited: a member can outlive it. A namespace not proven empty
/// keeps the scratch for the sweep.
fn retire(
    mut boundary: SpawnedBoundary,
    waited: std::io::Result<Option<ExitStatus>>,
    timeout: Duration,
) -> Result<(ExitStatus, u64, String), String> {
    let quiescence = kill_namespace(boundary.namespace_pid, boundary.pid_namespace);
    keep_scratch_unless_released(&mut boundary.scratch, &quiescence);
    boundary.process.retire();
    match waited {
        Ok(Some(status)) => {
            let stderr = super::super::launch::stderr_tail(boundary.stderr);
            Ok((status, boundary.pid_namespace, stderr))
        }
        Ok(None) => Err(format!(
            "the probe boundary did not exit within {} ms",
            timeout.as_millis()
        )),
        Err(error) => Err(format!("wait for the probe boundary: {error}")),
    }
}

#[cfg(test)]
#[path = "probe_host_tests.rs"]
mod tests;
