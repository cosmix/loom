//! Host qualification for native workers: a boundary with its own pid
//! namespace starts here, and bubblewrap nests inside it. Both answers are
//! measured once per process.
use std::sync::OnceLock;

/// `Ok` when this host can run native Codex workers: Linux, bubblewrap
/// installed, and a payload boundary runs in a pid namespace of its own.
pub fn native_backend_qualified() -> Result<(), String> {
    static QUALIFIED: OnceLock<Result<(), String>> = OnceLock::new();
    QUALIFIED.get_or_init(qualify).clone()
}

/// Whether bubblewrap can start inside a payload boundary (Codex's own
/// sandbox nests in it); false off Linux or without bubblewrap.
pub fn nested_sandbox_supported() -> bool {
    static NESTED: OnceLock<bool> = OnceLock::new();
    *NESTED.get_or_init(nested)
}

#[cfg(not(target_os = "linux"))]
fn qualify() -> Result<(), String> {
    Err("native Codex workers need Linux".into())
}

#[cfg(not(target_os = "linux"))]
fn nested() -> bool {
    false
}

#[cfg(target_os = "linux")]
fn qualify() -> Result<(), String> {
    which::which("bwrap").map_err(|_| "bubblewrap (bwrap) is not installed".to_string())?;
    let (status, namespace) = linux_probe::run(std::path::Path::new("true"), &[])?;
    if !status.success() {
        return Err(format!(
            "a bubblewrap boundary failed to run true ({status})"
        ));
    }
    let own = std::fs::read_link("/proc/self/ns/pid")
        .map_err(|error| format!("cannot read this process's pid namespace: {error}"))?;
    if own == std::path::Path::new(&format!("pid:[{namespace}]")) {
        return Err("the bubblewrap boundary shares this process's pid namespace".into());
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn nested() -> bool {
    let Some(bwrap) = which::which("bwrap")
        .ok()
        .and_then(|path| path.canonicalize().ok())
    else {
        return false;
    };
    let args = [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--ro-bind",
        "/",
        "/",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--",
        "true",
    ]
    .map(std::ffi::OsString::from);
    linux_probe::run(&bwrap, &args).is_ok_and(|(status, _)| status.success())
}

#[cfg(target_os = "linux")]
mod linux_probe {
    use std::ffi::OsString;
    use std::path::Path;
    use std::process::ExitStatus;
    use std::time::Duration;

    /// Run `program` in a payload boundary on a temporary worktree for at
    /// most 10 s; always retired. The exit status and the namespace inode.
    pub(super) fn run(program: &Path, args: &[OsString]) -> Result<(ExitStatus, u64), String> {
        let worktree = tempfile::tempdir().map_err(|error| format!("probe worktree: {error}"))?;
        let plan = super::super::linux::Plan::payload(worktree.path(), None, program, args)
            .map_err(|error| format!("{error:#}"))?;
        let host: Vec<_> = std::env::vars_os().collect();
        let mut boundary =
            super::super::linux::start(&plan, &host).map_err(|error| format!("{error:#}"))?;
        let waited = boundary.process.wait(Duration::from_secs(10));
        boundary.process.retire();
        match waited {
            Ok(Some(status)) => Ok((status, boundary.pid_namespace)),
            Ok(None) => Err("the probe boundary did not exit within 10 s".into()),
            Err(error) => Err(format!("wait for the probe boundary: {error}")),
        }
    }
}
