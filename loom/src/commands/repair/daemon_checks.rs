//! Daemon health — detect the singleton/socket failure modes documented in
//! concerns.md (2026-05-13). These are diagnostic-only: there is no safe
//! automatic fix (killing the wrong daemon loses orchestration state), so
//! each is reported with manual remediation guidance.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::daemon::{DaemonServer, DaemonStatus};
use crate::fs::work_dir::WorkDir;

use super::{RepairIssue, Severity};

/// Detect daemon singleton and socket failure modes for `repo_root`.
pub(super) fn check_daemon_health(repo_root: &Path) -> Vec<RepairIssue> {
    let work_dir = WorkDir::new(repo_root)
        .map(|wd| wd.root().to_path_buf())
        .unwrap_or_else(|_| repo_root.join(".loom").join("work"));
    if !work_dir.is_dir() {
        return Vec::new();
    }

    let mut issues = check_multiple_daemon_processes(&work_dir);
    issues.extend(check_daemon_socket_and_pid(&work_dir));
    issues
}

/// (1) More than one `loom run` process alive is wrong only when they serve
/// THIS repository — two plans in two different repositories each run their
/// own daemon, and each repository's `loom repair` must not flag the other's
/// as a duplicate.
fn check_multiple_daemon_processes(work_dir: &Path) -> Vec<RepairIssue> {
    let repo_root = derive_repo_root(work_dir);
    let lock_pid = DaemonServer::check_lock(work_dir);
    let processes = find_loom_run_processes();
    let run_pids = daemons_serving(&repo_root, &processes, lock_pid);
    if run_pids.len() <= 1 {
        return Vec::new();
    }
    let pid_list = run_pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let keep_hint = match lock_pid {
        Some(pid) => {
            format!("Keep the lock holder (PID {pid}); stop the others with `kill <pid>`")
        }
        None => "Stop the stale daemons with `kill <pid>` (no lock holder found)".to_string(),
    };
    vec![RepairIssue {
        severity: Severity::Critical,
        description: format!("Multiple 'loom run' processes alive (PIDs: {pid_list})"),
        fix_description: keep_hint,
    }]
}

/// Derive the repository root implied by a resolved `.loom/work` or legacy
/// `.work` state directory, without doing our own workspace search.
///
/// Strips a trailing `.loom`/`work` or `.work` path segment; if `work_dir`
/// ends in neither (an unexpected shape), its immediate parent is used
/// instead. The result is canonicalized so a repo root reached through a
/// symlink still compares equal to the canonical cwd `process_cwd` reports
/// for a live daemon; when canonicalization fails (the path does not exist,
/// e.g. in a unit test), the uncanonicalized candidate is kept so comparison
/// degrades gracefully rather than panicking.
fn derive_repo_root(work_dir: &Path) -> PathBuf {
    let is_work_component = |p: &Path| p.file_name() == Some(std::ffi::OsStr::new("work"));
    let is_loom_component = |p: &Path| p.file_name() == Some(std::ffi::OsStr::new(".loom"));

    // `.loom/work` strips both components to reach the repo root; the legacy
    // `.work` shape and any other, unexpected shape fall back to the
    // immediate parent.
    let candidate =
        if is_work_component(work_dir) && work_dir.parent().is_some_and(is_loom_component) {
            work_dir.parent().and_then(Path::parent)
        } else {
            work_dir.parent()
        }
        .unwrap_or(work_dir)
        .to_path_buf();

    candidate.canonicalize().unwrap_or(candidate)
}

/// A `loom run` process found by the scan, with the working directory it
/// runs in when that could be determined.
struct LoomRunProcess {
    pid: u32,
    cwd: Option<PathBuf>,
}

/// Enumerate currently-running `loom run` processes together with their
/// working directory, when it could be determined.
fn find_loom_run_processes() -> Vec<LoomRunProcess> {
    find_loom_run_pids()
        .into_iter()
        .map(|pid| LoomRunProcess {
            pid,
            cwd: process_cwd(pid),
        })
        .collect()
}

/// Filter `processes` down to the ones serving `repo_root`: a process whose
/// working directory canonicalizes to `repo_root`, or whose PID holds this
/// repository's daemon lock (a daemon that somehow changed directory after
/// locking still counts). A process whose cwd could not be determined is
/// dropped rather than assumed to match — the check degrades toward "no
/// duplicates", the same posture the `ps`-failure path in
/// `find_loom_run_pids` already takes.
fn daemons_serving(
    repo_root: &Path,
    processes: &[LoomRunProcess],
    lock_holder: Option<u32>,
) -> Vec<u32> {
    processes
        .iter()
        .filter(|p| p.cwd.as_deref() == Some(repo_root) || lock_holder == Some(p.pid))
        .map(|p| p.pid)
        .collect()
}

/// Look up the working directory of a running process.
///
/// Returns `None` on any failure (process gone, unsupported platform,
/// permission denied) rather than erroring — a daemon-health check must not
/// fail the whole `loom repair` run over a single unreadable process.
#[cfg(target_os = "linux")]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    let link = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
    Some(link.canonicalize().unwrap_or(link))
}

#[cfg(target_os = "macos")]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    let output = Command::new("lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `-Fn` output is one field per line; the cwd's name field is the first
    // line starting with `n`, with the marker stripped.
    let raw = stdout.lines().find_map(|line| line.strip_prefix('n'))?;
    let path = PathBuf::from(raw);
    Some(path.canonicalize().unwrap_or(path))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
}

/// (2)/(3) Lock held by a live daemon, but PID file or socket missing; or no
/// lock holder while a process still appears alive with an unreachable socket.
fn check_daemon_socket_and_pid(work_dir: &Path) -> Vec<RepairIssue> {
    let mut issues = Vec::new();
    if let Some(lock_pid) = DaemonServer::check_lock(work_dir) {
        if crate::process::is_process_alive(lock_pid) {
            let pid_path = work_dir.join("orchestrator.pid");
            let socket_path = work_dir.join("orchestrator.sock");

            if !pid_path.exists() {
                issues.push(RepairIssue {
                    severity: Severity::Warning,
                    description: format!(
                        "Daemon lock held (PID {lock_pid}) but orchestrator.pid is missing"
                    ),
                    fix_description:
                        "Restart the daemon: `loom stop`, then `loom run` (PID file was lost)"
                            .to_string(),
                });
            }

            // Raw `Path::exists()`, not `check_status`: still sees the socket when a sandboxed `connect()` is denied.
            if !socket_path.exists() {
                issues.push(RepairIssue {
                    severity: Severity::Critical,
                    description: format!(
                        "Daemon lock held (PID {lock_pid}) but orchestrator.sock is missing (daemon unreachable)"
                    ),
                    fix_description:
                        "Restart the daemon: `kill <pid>` then `loom run` (control socket was lost)"
                            .to_string(),
                });
            }
        }
    } else if DaemonServer::check_status(work_dir) == DaemonStatus::ProcessOnly {
        // No flock holder, but a process appears alive with an unreachable socket. Checked via `== ProcessOnly`, not `Unreachable` too: our own sandboxed `connect()` denial also reads as `Unreachable` and must not restart a healthy daemon.
        issues.push(RepairIssue {
            severity: Severity::Warning,
            description: "Daemon process appears alive but its socket is unreachable".to_string(),
            fix_description: "Restart the daemon: `loom stop`, then `loom run`".to_string(),
        });
    }
    issues
}

/// Enumerate the PIDs of currently-running `loom run` processes.
///
/// Uses `ps aux` (portable across Linux and macOS, matching the existing
/// process-scan pattern in `native/pid_tracking.rs`) and matches command lines
/// containing the `loom run` invocation, excluding this `loom repair` process.
/// On any `ps` failure returns an empty vec — the daemon-health checks degrade to
/// "no duplicates detected" rather than failing the whole repair run.
fn find_loom_run_pids() -> Vec<u32> {
    let our_pid = std::process::id();
    let output = match Command::new("ps")
        .arg("axww")
        .arg("-o")
        .arg("pid=,args=")
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut pids = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_start();
        let mut parts = line.splitn(2, char::is_whitespace);
        let pid: u32 = match parts.next().and_then(|p| p.trim().parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        let args = parts.next().unwrap_or("");
        if pid == our_pid {
            continue;
        }
        // Match the `loom run` invocation. Require the program component to end in
        // `loom` and the next token to be `run` so unrelated commands that merely
        // mention the words (e.g. an editor on this file) are not counted.
        if is_loom_run_cmdline(args) {
            pids.push(pid);
        }
    }
    pids
}

/// Return true if `args` is a `loom run ...` command line.
pub(super) fn is_loom_run_cmdline(args: &str) -> bool {
    let mut tokens = args.split_whitespace();
    let Some(program) = tokens.next() else {
        return false;
    };
    // The program token may be a path like `/usr/local/bin/loom` or `loom`.
    let prog_name = program.rsplit('/').next().unwrap_or(program);
    if prog_name != "loom" {
        return false;
    }
    tokens.next() == Some("run")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, cwd: Option<&Path>) -> LoomRunProcess {
        LoomRunProcess {
            pid,
            cwd: cwd.map(Path::to_path_buf),
        }
    }

    /// Two daemons in two different repositories: only the one whose cwd
    /// matches `repo_root` is reported, regardless of scan order.
    #[test]
    fn daemons_serving_keeps_only_the_matching_cwd() {
        let repo_a = Path::new("/repos/a");
        let repo_b = Path::new("/repos/b");
        let processes = vec![process(101, Some(repo_a)), process(202, Some(repo_b))];

        let matched = daemons_serving(repo_a, &processes, None);

        assert_eq!(matched, vec![101]);
    }

    /// An unrelated repo's daemon alongside this repo's own must not push
    /// the count over one — this is the exact bug: `loom repair` in either
    /// repo must see only its own daemon.
    #[test]
    fn daemons_serving_does_not_count_an_unrelated_repos_daemon() {
        let this_repo = Path::new("/repos/this");
        let other_repo = Path::new("/repos/other");
        let processes = vec![process(11, Some(this_repo)), process(22, Some(other_repo))];

        let matched = daemons_serving(this_repo, &processes, None);

        assert_eq!(matched, vec![11]);
    }

    /// A process whose cwd could not be determined is dropped, not assumed
    /// to match — the check degrades toward "no duplicates".
    #[test]
    fn daemons_serving_drops_a_process_with_unknown_cwd() {
        let repo_root = Path::new("/repos/this");
        let processes = vec![process(11, Some(repo_root)), process(99, None)];

        let matched = daemons_serving(repo_root, &processes, None);

        assert_eq!(matched, vec![11]);
    }

    /// A process holding this repo's daemon lock counts even when its cwd
    /// could not be determined — the lock is authoritative.
    #[test]
    fn daemons_serving_keeps_the_lock_holder_despite_unknown_cwd() {
        let repo_root = Path::new("/repos/this");
        let processes = vec![process(11, Some(repo_root)), process(99, None)];

        let matched = daemons_serving(repo_root, &processes, Some(99));

        assert_eq!(matched, vec![11, 99]);
    }

    /// `.loom/work` strips both trailing components to reach the repo root.
    #[test]
    fn derive_repo_root_strips_nested_loom_work() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().canonicalize().unwrap();
        let work_dir = repo_root.join(".loom").join("work");
        std::fs::create_dir_all(&work_dir).unwrap();

        assert_eq!(derive_repo_root(&work_dir), repo_root);
    }

    /// Legacy `.work` strips a single trailing component.
    #[test]
    fn derive_repo_root_strips_legacy_work() {
        let dir = tempfile::tempdir().unwrap();
        let repo_root = dir.path().canonicalize().unwrap();
        let work_dir = repo_root.join(".work");
        std::fs::create_dir_all(&work_dir).unwrap();

        assert_eq!(derive_repo_root(&work_dir), repo_root);
    }
}
