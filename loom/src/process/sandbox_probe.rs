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

mod launcher;
mod needs;
mod orphan;
pub mod reap;

pub use launcher::start_blocker;
pub use needs::{
    bash_process_substitution_usable, plan_locks_usable, skip_without_confinement,
    skip_without_lockfile_access, skip_without_loopback, skip_without_plan_locks,
    skip_without_process_substitution, skip_without_process_tree, skip_without_unix_socket,
};
#[cfg(test)]
pub(crate) use needs::{require_unix_socket, require_unix_socket_and_loopback};
pub use orphan::{orphans_signalable, skip_without_orphan_signals, spawn_orphan};

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
/// `loom-hooks/_common.sh::loom_proc_tree_available` exactly, since both exist to
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
/// Memoized like [`process_tree_visible`]: a fact about the sandbox, not
/// about any one test.
pub fn loopback_bindable() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| std::net::TcpListener::bind("127.0.0.1:0").is_ok())
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
/// `fs::permissions::settings::codex_forward_home_allow_entry` returns `None`
/// when the home directory can't be determined (callers skip the entry, never
/// fail), so a test that asserts the entry's presence or removal needs this
/// fact separated out rather than panicking on the same `None`.
pub fn home_dir_resolvable() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| dirs::home_dir().is_some())
}

/// Whether a confined command can start here: `bwrap` on Linux, using the
/// namespace and mount options the verification runner relies on, or
/// `sandbox-exec` on macOS. A session sandbox that already nests the launcher
/// (bubblewrap, or a Seatbelt profile that differs from the outer one), or
/// forbids user namespaces, makes every confined verification command fail
/// regardless of the code under test. Always true on other platforms. Memoized.
///
/// When it is false, every call goes through `skip_unless`: it prints a `SKIP`
/// line naming why (the launcher missing, or failing with its exit status and
/// first stderr line), or panics under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` so CI
/// cannot pass with these tests silently skipped. Callers return on false.
pub fn confined_commands_runnable() -> bool {
    match confined_commands_blocker() {
        None => true,
        Some(why) => {
            skip_unless(false, "confined commands", &format!("cannot start: {why}"));
            false
        }
    }
}

/// Why confined commands cannot start here (the launcher missing, or failing
/// with its exit status and first stderr line), or `None` when they can. Memoized;
/// prints nothing, for callers that report the reason themselves.
pub fn confined_commands_blocker() -> Option<&'static str> {
    static RESULT: OnceLock<Result<(), String>> = OnceLock::new();
    RESULT
        .get_or_init(confined_command_blocker)
        .as_ref()
        .err()
        .map(String::as_str)
}

/// Start one confined `true` the way this platform's launcher does (`bwrap` on
/// Linux, `sandbox-exec` on macOS); `Err` names why it could not. Off those
/// platforms confinement is not probed.
fn confined_command_blocker() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let args = [
            "--die-with-parent",
            "--unshare-all",
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
        ];
        match start_blocker(Path::new("bwrap"), &args) {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let launcher = Path::new("/usr/bin/sandbox-exec");
        if !launcher.is_file() {
            return Err(format!("{} is missing", launcher.display()));
        }
        // A nested Seatbelt profile that differs from the outer one is refused
        // with `sandbox_apply: Operation not permitted`.
        let args = ["-p", "(version 1)(allow default)", "/usr/bin/true"];
        match start_blocker(launcher, &args) {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Ok(())
    }
}

/// Whether this process can kill the process group of a child it started. A
/// host job's Seatbelt profile may refuse it, and then tests that expect loom
/// to retire a process group cannot run there. Memoized.
///
/// The child is `cat` on a pipe, so closing its stdin ends it when the kill is
/// refused: the reap never depends on the signal it probes.
pub fn process_groups_signalable() -> bool {
    use std::os::unix::process::CommandExt;
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| {
        let spawned = std::process::Command::new("/bin/cat")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .process_group(0)
            .spawn();
        let Ok(mut child) = spawned else {
            return false;
        };
        let group = nix::unistd::Pid::from_raw(child.id() as i32);
        let signalled = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL).is_ok();
        drop(child.stdin.take());
        reap::reap_within(&mut child, reap::REAP_DEADLINE);
        signalled
    })
}

/// Whether this process can allocate a pseudo-terminal pair. A host job's
/// profile may refuse the `/dev/ptmx` open. Opens and closes one; memoized.
pub fn pty_allocatable() -> bool {
    static RESULT: OnceLock<bool> = OnceLock::new();
    *RESULT.get_or_init(|| nix::pty::openpty(None, None).is_ok())
}

/// Skip decision for a test that retires a process group: true when the caller
/// should return because this process cannot signal a child's group. Call it
/// before the test spawns anything, since a guard that cannot kill blocks on
/// `wait`.
pub fn skip_without_group_signals(test_name: &str) -> bool {
    skip_unless(
        process_groups_signalable(),
        test_name,
        "this process cannot signal a child's process group",
    )
}

/// Skip decision for a test that lists the process table and retires what it
/// spawned: true when `ps` is refused or the child's group cannot be signalled.
/// Call it before the test spawns anything.
pub fn skip_without_process_control(test_name: &str) -> bool {
    skip_unless(
        process_tree_visible(),
        test_name,
        "this process cannot list the process table",
    ) || skip_without_group_signals(test_name)
}

/// Skip decision for a test that needs a PTY: true when none can be allocated.
pub fn skip_without_pty(test_name: &str) -> bool {
    skip_unless(
        pty_allocatable(),
        test_name,
        "this process cannot allocate a PTY",
    )
}

/// Returns from the calling test, naming it, when this process cannot signal a
/// child's process group. Put it first in a test that spawns a stand-in it later
/// retires by signal; one line keeps the test under the function size limit.
#[cfg(test)]
macro_rules! require_group_signals {
    ($test:literal) => {
        if $crate::process::sandbox_probe::skip_without_group_signals($test) {
            return;
        }
    };
}
#[cfg(test)]
pub(crate) use require_group_signals;

/// Like [`require_group_signals`], and also returns when `ps` is refused, for a
/// test that reads the process table of the stand-in it spawned.
#[cfg(test)]
macro_rules! require_process_control {
    ($test:literal) => {
        if $crate::process::sandbox_probe::skip_without_process_control($test) {
            return;
        }
    };
}
#[cfg(test)]
pub(crate) use require_process_control;

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
#[path = "host_job_lane_tests.rs"]
mod host_job_lane;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_unless_never_skips_when_the_probe_passed() {
        assert!(!skip_unless(true, "probe_test", "should not skip"));
    }

    #[test]
    fn skip_unless_obeys_policy_when_the_probe_failed() {
        let strict = std::env::var("LOOM_TEST_REQUIRE_SANDBOX_FREE").as_deref() == Ok("1");
        let result = std::panic::catch_unwind(|| skip_unless(false, "probe_test", "unavailable"));
        if strict {
            let panic = result.expect_err("strict mode must refuse a failed probe");
            assert_eq!(
                panic.downcast_ref::<String>().map(String::as_str),
                Some(
                    "probe_test: unavailable (LOOM_TEST_REQUIRE_SANDBOX_FREE=1 demands a real run)"
                )
            );
        } else {
            assert!(result.expect("default mode must return the skip decision"));
        }
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
