//! Host qualification for native workers: bubblewrap can bound a tmpfs, a
//! boundary with its own pid namespace starts here, and Codex's own sandbox
//! runs inside it. The answer is measured once per process.
use std::sync::OnceLock;

#[cfg(target_os = "linux")]
#[path = "probe_host.rs"]
mod host;

/// Why the lane is unavailable when bubblewrap has no `--size` option.
#[cfg(target_os = "linux")]
const SIZE_REQUIRED: &str =
    "bubblewrap 0.10 or later required: the boundary bounds its /tmp and /var/tmp with --size";

/// Why the lane is unavailable when Codex's sandbox cannot run in the boundary.
/// Codex's own sandbox (seccomp and a private network namespace) is what keeps
/// a command off the loopback proxy port and the proxy socket; without it
/// every command could reach the allowlisted domains, and nothing the
/// app-server holds can gate the proxy, since a same-user command in its pid
/// namespace reads the app-server's environment from `/proc`.
#[cfg(target_os = "linux")]
const NESTING_REQUIRED: &str =
    "Codex's command sandbox, which keeps commands off the proxy, cannot run inside the boundary";

/// `Ok` when this host can run native Codex workers: Linux, bubblewrap
/// installed with `--size`, a payload boundary runs in a pid namespace of its
/// own, and Codex's own sandbox starts inside it.
pub fn native_backend_qualified() -> Result<(), String> {
    static QUALIFIED: OnceLock<Result<(), String>> = OnceLock::new();
    QUALIFIED.get_or_init(qualify).clone()
}

#[cfg(not(target_os = "linux"))]
fn qualify() -> Result<(), String> {
    Err("native Codex workers need Linux".into())
}

#[cfg(target_os = "linux")]
fn qualify() -> Result<(), String> {
    qualify_from(&host::RealHost)
}

/// What qualification measures on a host, in the order it is asked.
#[cfg(target_os = "linux")]
trait HostProbes {
    /// The host bubblewrap's `--help` text.
    fn bwrap_help(&self) -> Result<String, String>;
    /// A payload boundary running `true`: its exit status and pid namespace.
    fn boundary(&self) -> Result<(std::process::ExitStatus, u64), String>;
    /// This process's pid namespace link, `pid:[<inode>]`.
    fn own_pid_namespace(&self) -> Result<std::path::PathBuf, String>;
    /// Codex's own sandbox, run inside a payload boundary.
    fn nesting(&self) -> Result<Nesting, String>;
}

/// How Codex's sandbox fared inside a payload boundary.
#[cfg(target_os = "linux")]
#[derive(Debug)]
struct Nesting {
    /// The sandboxed `true` succeeded.
    ran: bool,
    /// The bubblewrap Codex's sandbox uses there, when one is visible.
    bwrap: Option<std::path::PathBuf>,
    /// The start of the probe's stderr.
    stderr: String,
}

/// Every gate, in order, over measured `probes`.
#[cfg(target_os = "linux")]
fn qualify_from(probes: &impl HostProbes) -> Result<(), String> {
    tmpfs_size_gate(&probes.bwrap_help()?)?;
    let (status, namespace) = probes.boundary()?;
    if !status.success() {
        return Err(format!(
            "a bubblewrap boundary failed to run true ({status})"
        ));
    }
    let own = probes.own_pid_namespace()?;
    if own == std::path::Path::new(&format!("pid:[{namespace}]")) {
        return Err("the bubblewrap boundary shares this process's pid namespace".into());
    }
    nesting_gate(&probes.nesting()?)
}

/// `Ok` when `bwrap --help` lists `--size` (bubblewrap 0.10 or later).
#[cfg(target_os = "linux")]
fn tmpfs_size_gate(help: &str) -> Result<(), String> {
    let listed = help
        .lines()
        .any(|line| line.trim_start().starts_with("--size "));
    listed
        .then_some(())
        .ok_or_else(|| SIZE_REQUIRED.to_string())
}

/// `Ok` when Codex's sandbox ran; otherwise the refusal names the bubblewrap
/// it used, or that none was visible.
#[cfg(target_os = "linux")]
fn nesting_gate(nesting: &Nesting) -> Result<(), String> {
    if nesting.ran {
        return Ok(());
    }
    let tried = match &nesting.bwrap {
        Some(path) => format!("it runs bubblewrap {} there", path.display()),
        None => "no bubblewrap is visible there, neither on the boundary's PATH nor as \
                 the codex release's codex-resources/bwrap"
            .to_string(),
    };
    let stderr = nesting.stderr.trim();
    let said = if stderr.is_empty() {
        String::new()
    } else {
        format!("; codex sandbox stderr: {stderr}")
    };
    Err(format!("{NESTING_REQUIRED}: {tried}{said}"))
}

#[cfg(all(test, target_os = "linux"))]
#[path = "probe_tests.rs"]
mod tests;
