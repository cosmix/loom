//! Membership of a pid namespace, read from `/proc/<pid>/ns/pid`, which
//! resolves across a boundary's user namespace. Shared by the native-worker
//! quiescence check and the command-scratch sweep, which must not remove a
//! scratch still bound into a live boundary.
use std::io::ErrorKind;
use std::path::PathBuf;

/// Live (non-zombie) pids whose pid namespace is `namespace`; `Err` when
/// some process could be a member but cannot be inspected.
pub(crate) fn members(namespace: u64) -> Result<Vec<u32>, String> {
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

/// Whether a process whose namespace link is unreadable is provably not a
/// member. Members run under this uid (a boundary maps only it), and they
/// live in a child pid namespace, so `NSpid` lists more than one pid. A
/// non-dumpable member shows `/proc/<pid>` owned by root, so the directory
/// owner alone proves nothing; `status` stays readable.
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
