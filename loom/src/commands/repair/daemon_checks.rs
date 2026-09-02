//! Daemon health — detect the singleton/socket failure modes documented in
//! concerns.md (2026-05-13). These are diagnostic-only: there is no safe
//! automatic fix (killing the wrong daemon loses orchestration state), so
//! each is reported with manual remediation guidance.

use std::path::Path;

use crate::daemon::{DaemonServer, DaemonStatus};
use crate::fs::work_dir::WorkDir;

use super::{find_loom_run_pids, RepairIssue, Severity};

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

/// (1) More than one `loom run` process alive is always wrong.
fn check_multiple_daemon_processes(work_dir: &Path) -> Vec<RepairIssue> {
    let run_pids = find_loom_run_pids();
    if run_pids.len() <= 1 {
        return Vec::new();
    }
    let pid_list = run_pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let lock_pid = DaemonServer::check_lock(work_dir);
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
