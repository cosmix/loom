//! Ownership check after a boundary stopped: no process of its pid namespace
//! may be left (`verify::criteria::pid_namespace` reads the membership).
use super::{Quiescence, SpawnedBoundary};
use crate::native_worker::model::BoundaryIdentity;

/// The identity under which the supervisor records `boundary`.
pub(crate) fn identity_for(boundary: &SpawnedBoundary) -> anyhow::Result<BoundaryIdentity> {
    use anyhow::Context;
    let supervisor_pid = std::process::id();
    let supervisor_start =
        start_time(supervisor_pid).context("cannot read the supervisor's process start time")?;
    Ok(BoundaryIdentity {
        supervisor_pid: supervisor_pid as i32,
        supervisor_start,
        boundary_pid: boundary.boundary_pid,
        pid_namespace: boundary.pid_namespace,
        boot_id: crate::process::boot_id::os_boot_id()?,
    })
}

#[cfg(target_os = "linux")]
fn start_time(pid: u32) -> Option<String> {
    crate::process::process_start_time(pid).map(|time| time.to_string())
}

#[cfg(not(target_os = "linux"))]
fn start_time(_: u32) -> Option<String> {
    None
}

/// `Released` when the boot id changed (every process died with the old
/// boot) or no live process is left in the recorded pid namespace within
/// `REAP_GRACE_SECS`; otherwise `Unverified` with the reason.
pub fn verify_quiescent(identity: &BoundaryIdentity) -> Quiescence {
    let Ok(boot_id) = crate::process::boot_id::os_boot_id() else {
        return Quiescence::Unverified("cannot read the boot id".into());
    };
    if boot_id != identity.boot_id {
        return Quiescence::Released;
    }
    scan_until_empty(identity.pid_namespace)
}

#[cfg(not(target_os = "linux"))]
fn scan_until_empty(_: u64) -> Quiescence {
    Quiescence::Unverified("pid namespaces need Linux".into())
}

#[cfg(target_os = "linux")]
fn scan_until_empty(namespace: u64) -> Quiescence {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(crate::native_worker::ids::REAP_GRACE_SECS);
    loop {
        let last = match crate::verify::criteria::pid_namespace::members(namespace) {
            Ok(members) if members.is_empty() => return Quiescence::Released,
            Ok(members) => {
                let shown: Vec<String> = members.iter().take(5).map(u32::to_string).collect();
                format!(
                    "{} process(es) still in pid namespace {namespace}: pids {}",
                    members.len(),
                    shown.join(", ")
                )
            }
            Err(detail) => detail,
        };
        if Instant::now() >= deadline {
            return Quiescence::Unverified(last);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// SIGKILL `first_pid`, the first process of `namespace` as this side sees
/// it, which takes every member down with it, then wait as
/// [`verify_quiescent`] does for the namespace to empty.
#[cfg(target_os = "linux")]
pub(crate) fn kill_namespace(first_pid: i32, namespace: u64) -> Quiescence {
    kill_member(first_pid, namespace);
    scan_until_empty(namespace)
}

/// SIGKILL `pid` only while it is still a member of `namespace`. The
/// `/proc/{pid}` namespace read happens while the pidfd pins the pid: the
/// kernel reuses no pid while the pinned process or its zombie exists, so the
/// read sees that process or fails. A successful `pidfd_send_signal` then
/// proves the pinned process still existed, so the process signalled is the
/// one whose membership was read; a pid reused before `pidfd_open` fails the
/// membership check instead. That check names the namespace only by inode,
/// so one window remains: the namespace's first process dies, its inode is
/// reused by another boundary's namespace, and `pid` is reused by a process
/// of that namespace, all before `pidfd_open`. That process is then killed.
#[cfg(target_os = "linux")]
fn kill_member(pid: i32, namespace: u64) {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    // SAFETY: pidfd_open takes a pid and flags; it returns a new descriptor or -1.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    let Ok(raw) = i32::try_from(raw) else {
        return;
    };
    if raw < 0 {
        return;
    }
    // SAFETY: `raw` is a descriptor pidfd_open just returned, owned only here.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    let target = std::path::PathBuf::from(format!("pid:[{namespace}]"));
    if std::fs::read_link(format!("/proc/{pid}/ns/pid")).is_ok_and(|link| link == target) {
        let null = std::ptr::null::<libc::siginfo_t>();
        // SAFETY: a valid pidfd, a valid signal, no siginfo and no flags.
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                pidfd.as_raw_fd(),
                libc::SIGKILL,
                null,
                0,
            )
        };
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "quiescence_tests.rs"]
mod tests;
