//! Process-global test guards for the tmux backend e2e tests.
//!
//! Prefer a short `/tmp` socket path; fall back only when it is unwritable and
//! the projected path fits the macOS/BSD limit. Bind permission is probed separately.
//! See [tmux backend notes](../../../doc/loom/knowledge/mistakes/tmux-backend.md).

use loom::orchestrator::terminal::tmux::kill_socket_server;
use std::path::{Path, PathBuf};

/// Restores a process env var to its previous value on drop, on EVERY exit
/// path including a panic -- so overriding a process-global var (like
/// `TMUX_TMPDIR` below) can never leak a stale value into whichever test the
/// harness runs next.
pub(crate) struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

/// The stricter of the two platform `AF_UNIX sun_path` limits (104 bytes on
/// macOS/BSD, 108 on Linux) -- used so a length check passing here holds on
/// every platform this suite runs on.
const SUN_PATH_LIMIT: usize = 104;

/// Bytes reserved for the socket's own final path component,
/// `loom-<session-id>`. Real session ids look like
/// `session-6d4cf60c-1786963954`, so this leaves headroom for the `loom-`
/// prefix plus a realistic id rather than measuring one exactly.
const SOCKET_NAME_BUDGET: usize = 40;

/// Select a writable base whose projected tmux socket path fits `sun_path`.
/// Fail rather than skip when no candidate is usable.
fn create_isolated_tmux_tmpdir() -> PathBuf {
    // SAFETY: getuid() is always safe to call and cannot fail. Matches
    // `loom_socket_dir()` in `src/orchestrator/terminal/tmux/socket.rs`,
    // which builds the real socket path the same way.
    let uid = unsafe { libc::getuid() };
    let mut rejected = Vec::new();
    let mut tried = Vec::new();

    for base in [PathBuf::from("/tmp"), std::env::temp_dir()] {
        let dir = base.join(format!("lt-{}", std::process::id()));
        // `std::env::temp_dir()` falls back to `/tmp` when `$TMPDIR` is
        // unset, so the two candidates can coincide -- skip a repeat rather
        // than re-trying (and re-reporting) the identical path.
        if tried.contains(&dir) {
            continue;
        }
        tried.push(dir.clone());

        if let Err(err) = std::fs::create_dir_all(&dir) {
            rejected.push(format!("{} (unwritable: {err})", dir.display()));
            continue;
        }

        let projected_len =
            dir.display().to_string().len() + format!("/tmux-{uid}/").len() + SOCKET_NAME_BUDGET;
        if projected_len > SUN_PATH_LIMIT {
            rejected.push(format!(
                "{} (projected socket path {projected_len} bytes exceeds sun_path limit {SUN_PATH_LIMIT})",
                dir.display()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            continue;
        }

        return dir;
    }

    panic!("no usable TMUX_TMPDIR base found; rejected candidates: {rejected:?}");
}

/// Redirects `TMUX_TMPDIR` to an isolated per-test directory for the
/// duration of the guard, additionally removing the directory on drop -- on
/// EVERY exit path, including a panic mid-test.
pub(crate) struct TmuxTmpDirGuard {
    _env: EnvVarGuard,
    dir: PathBuf,
}

impl TmuxTmpDirGuard {
    pub(crate) fn new() -> Self {
        let dir = create_isolated_tmux_tmpdir();
        let _env = EnvVarGuard::set("TMUX_TMPDIR", &dir);
        Self { _env, dir }
    }

    /// The isolated `TMUX_TMPDIR` this guard set, e.g. for a bind probe
    /// before starting a real tmux server against it.
    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TmuxTmpDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Tears down a tmux server started for a test on EVERY exit path, including
/// a panicking assertion mid-test -- without this, a failing assertion
/// between spawn and an explicit teardown call would strand a live tmux
/// server outside the test's own `TMUX_TMPDIR` cleanup.
pub(crate) struct TmuxServerGuard {
    pub(crate) socket_path: PathBuf,
}

impl Drop for TmuxServerGuard {
    fn drop(&mut self) {
        kill_socket_server(&self.socket_path);
        // Bounded wait for the server to exit: it is gone once its socket
        // refuses a `list-sessions` probe.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            let alive = std::process::Command::new("tmux")
                .arg("-S")
                .arg(&self.socket_path)
                .arg("list-sessions")
                .env_remove("TMUX")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !alive {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
