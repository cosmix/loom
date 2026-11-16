//! Ownership check after a boundary stopped: no process of its pid namespace
//! may be left. Membership is read from `/proc/<pid>/ns/pid`, which resolves
//! across the boundary's user namespace.
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
        let last = match scan::members(namespace) {
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

#[cfg(target_os = "linux")]
mod scan {
    use std::io::ErrorKind;
    use std::path::PathBuf;

    /// Live (non-zombie) pids whose pid namespace is `namespace`; `Err`
    /// when some process could be a member but cannot be inspected.
    pub(super) fn members(namespace: u64) -> Result<Vec<u32>, String> {
        let target = PathBuf::from(format!("pid:[{namespace}]"));
        let entries =
            std::fs::read_dir("/proc").map_err(|error| format!("cannot list /proc: {error}"))?;
        let mut members = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot list /proc: {error}"))?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            match std::fs::read_link(format!("/proc/{pid}/ns/pid")) {
                Ok(link) if link == target => {
                    if !crate::process::process_is_zombie(pid) {
                        members.push(pid);
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) if error.kind() == ErrorKind::PermissionDenied => {
                    if !cannot_be_member(pid) {
                        return Err(format!(
                            "cannot read the pid namespace of pid {pid}: {error}"
                        ));
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "cannot read the pid namespace of pid {pid}: {error}"
                    ));
                }
            }
        }
        Ok(members)
    }

    /// Whether a process whose namespace link is unreadable is provably not
    /// a member. Members run under this uid (the boundary maps only it), and
    /// they live in a child pid namespace, so `NSpid` lists more than one
    /// pid. A non-dumpable member shows `/proc/<pid>` owned by root, so the
    /// directory owner alone proves nothing; `status` stays readable.
    fn cannot_be_member(pid: u32) -> bool {
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
            // Gone since the listing: an exited process is not a member.
            return std::fs::metadata(format!("/proc/{pid}"))
                .is_err_and(|error| error.kind() == ErrorKind::NotFound);
        };
        // SAFETY: `getuid` has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        let field = |name: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .map(|rest| rest.split_whitespace().collect::<Vec<_>>())
        };
        let foreign = field("Uid:").is_some_and(|ids| {
            ids.iter()
                .take(2)
                .all(|id| id.parse::<u32>().is_ok_and(|id| id != uid))
        });
        let own_namespace = field("NSpid:").is_some_and(|pids| pids.len() == 1);
        foreign || own_namespace
    }
}
