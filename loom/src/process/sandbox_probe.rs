//! Environment probes for tests that cannot pass inside a sandbox withholding
//! process-tree visibility, `AF_UNIX` or loopback TCP binding, filesystem writes, or a
//! resolvable home directory, for reasons that have nothing to do with the
//! code under test.
//!
//! Mirrors `tests/e2e/tmux_backend.rs`'s `skip_unless_tmux_can_bind`: a test
//! that depends on one of these facts starts with a guard naming the probe
//! and returns early, printing a loud `SKIP` line instead of failing for an
//! environmental reason no assertion can distinguish from a real regression.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// A per-directory answer cache, backed by a `OnceLock`-initialized map so
/// each unique `dir` a probe is asked about is only ever measured once.
type PathProbeCache = OnceLock<Mutex<HashMap<PathBuf, bool>>>;

fn cached_path_probe(
    cache: &'static PathProbeCache,
    dir: &Path,
    probe: impl FnOnce(&Path) -> bool,
) -> bool {
    let cache = cache.get_or_init(|| Mutex::new(HashMap::new()));
    let mut answers = cache.lock().expect("sandbox probe cache poisoned");
    if let Some(&answer) = answers.get(dir) {
        return answer;
    }
    let answer = probe(dir);
    answers.insert(dir.to_path_buf(), answer);
    answer
}

/// Whether `ps -o ppid=` can read this process's own parent pid. Mirrors
/// `hooks/_common.sh::loom_proc_tree_available` exactly, since both exist to
/// explain the SAME class of failure: a sandbox that denies `ps`/`/proc`
/// makes every ancestry or process-status check in this codebase degrade the
/// same way, whether it walks a chain of pids or reads a single child's
/// state.
#[cfg(target_os = "linux")]
fn own_parent_pid_visible() -> bool {
    std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| stat.split_whitespace().nth(3).map(|_| ()))
        .is_some()
}

#[cfg(not(target_os = "linux"))]
fn own_parent_pid_visible() -> bool {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid])
        .output()
        .map(|out| out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
        .unwrap_or(false)
}

/// Returns true if this process can see its own parent pid through the OS
/// process-tree interface. Memoized: the answer is a fact about the sandbox,
/// not about any one test, and several of the 22 tests this module exists
/// for call it independently within the same test binary.
pub fn process_tree_visible() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(own_parent_pid_visible)
}

/// Whether `ps -o pgid=` (or, on Linux, `/proc/self/stat` field 5) can read
/// this process's own process-group id. Distinct from
/// [`process_tree_visible`]: that probe answers whether the OS exposes
/// parent-pid ancestry, this one answers whether it exposes the process
/// GROUP id specifically - a sandbox can allow one while denying the other.
#[cfg(target_os = "linux")]
fn own_process_group_visible() -> bool {
    std::fs::read_to_string("/proc/self/stat")
        .ok()
        .and_then(|stat| stat.split_whitespace().nth(4).map(|_| ()))
        .is_some()
}

#[cfg(not(target_os = "linux"))]
fn own_process_group_visible() -> bool {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "pgid=", "-p", &pid])
        .output()
        .map(|out| out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().is_empty())
        .unwrap_or(false)
}

/// Returns true if this process can see its own process-group id through the
/// OS process-status interface. Memoized like [`process_tree_visible`].
pub fn process_group_visible() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(own_process_group_visible)
}

/// Returns true if this process may bind an `AF_UNIX` listener under `dir`.
///
/// Binding and connecting are governed separately on macOS: inside the
/// Claude Code Bash sandbox (Seatbelt), `connect` behaves like an
/// unsandboxed process, but `bind` fails outright with `PermissionDenied`
/// (see `daemon/rpc.rs`'s own `af_unix_bind_available` test helper, which
/// this mirrors as a reusable probe).
pub fn unix_socket_bindable(dir: &Path) -> bool {
    static CACHE: PathProbeCache = OnceLock::new();
    cached_path_probe(&CACHE, dir, |dir| {
        let probe_path = dir.join(format!("sandbox-probe-{}.sock", std::process::id()));
        let bound = std::os::unix::net::UnixListener::bind(&probe_path).is_ok();
        let _ = std::fs::remove_file(&probe_path);
        bound
    })
}

/// Whether this process may bind a TCP listener on the loopback interface.
pub fn loopback_bindable() -> bool {
    std::net::TcpListener::bind("127.0.0.1:0").is_ok()
}

/// Returns true if this process may create and remove a file under `dir`.
pub fn path_writable(dir: &Path) -> bool {
    static CACHE: PathProbeCache = OnceLock::new();
    cached_path_probe(&CACHE, dir, |dir| {
        let probe_path = dir.join(format!(".sandbox-probe-{}", std::process::id()));
        match std::fs::write(&probe_path, b"probe") {
            Ok(()) => {
                let _ = std::fs::remove_file(&probe_path);
                true
            }
            Err(_) => false,
        }
    })
}

/// Returns true if `dirs::home_dir()` resolves in this environment.
///
/// `fs::permissions::settings::codex_forward_home_allow_entry` silently skips
/// its dynamic permission entry when the home directory can't be determined
/// (never a hard failure — see that function's doc comment), so a test that
/// asserts the entry's presence needs this fact separated out rather than
/// panicking on the same `None` the code under test already handles.
pub fn home_dir_resolvable() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| dirs::home_dir().is_some())
}

/// Returns true if the caller should skip: `probe_ok` is false, so the
/// environment cannot support what `test_name` needs, for reasons that have
/// nothing to do with the code under test. Prints a loud `SKIP` line naming
/// `why`. Panics instead of skipping when `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`,
/// so CI can demand a real run.
pub fn skip_unless(probe_ok: bool, test_name: &str, why: &str) -> bool {
    if probe_ok {
        return false;
    }
    if std::env::var("LOOM_TEST_REQUIRE_SANDBOX_FREE").as_deref() == Ok("1") {
        panic!("{test_name}: {why} (LOOM_TEST_REQUIRE_SANDBOX_FREE=1 demands a real run)");
    }
    eprintln!("SKIP {test_name}: {why} (set LOOM_TEST_REQUIRE_SANDBOX_FREE=1 to fail instead)");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_unless_never_skips_when_the_probe_passed() {
        assert!(!skip_unless(true, "probe_test", "should not skip"));
    }

    #[test]
    fn skip_unless_skips_when_the_probe_failed() {
        assert!(skip_unless(false, "probe_test", "should skip"));
    }

    #[test]
    fn path_writable_is_true_for_a_real_directory() {
        let dir = tempfile::tempdir().expect("create temp dir");
        assert!(path_writable(dir.path()));
    }

    #[test]
    fn path_writable_is_false_for_a_nonexistent_path() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let missing = dir.path().join("no-such-parent").join("no-such-file");
        assert!(!path_writable(&missing));
    }

    #[test]
    fn process_group_visible_returns_a_bool_without_panicking() {
        let _ = process_group_visible();
    }
}
