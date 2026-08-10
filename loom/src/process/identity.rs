//! Process identity verification for destructive process operations.

use anyhow::{Context, Result};
use std::fmt;

/// Persisted identity of a process Loom started.
///
/// A PID alone is never an identity: operating systems recycle PID numbers.
/// `start_time` is therefore required before Loom may signal the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub start_time: Option<u64>,
}

/// Result of comparing persisted identity evidence with the current process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityStatus {
    /// The PID exists and its start time matches the persisted value.
    VerifiedAlive,
    /// The PID no longer exists, or now has a different start time.
    Dead,
    /// The process exists, but either persisted or current start time is absent.
    Unverifiable,
}

/// A destructive operation was refused because process identity was incomplete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnverifiedProcessIdentity {
    pid: u32,
}

impl UnverifiedProcessIdentity {
    pub fn new(pid: u32) -> Self {
        Self { pid }
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl fmt::Display for UnverifiedProcessIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "refusing to signal PID {} without matching process start-time evidence",
            self.pid
        )
    }
}

impl std::error::Error for UnverifiedProcessIdentity {}

/// Read the kernel process start-time token used to detect PID reuse.
#[cfg(target_os = "linux")]
pub fn process_start_time(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(") ")?.1;
    after_comm.split_whitespace().nth(19)?.parse().ok()
}

/// Whether `pid` has exited but not yet been reaped by its parent.
///
/// A zombie keeps its PID entry — and its start-time token — until the parent
/// waits on it, so `kill(pid, 0)` and [`process_start_time`] both still answer
/// as though it were running. It cannot execute anything, so every liveness
/// question loom asks about it ("is this session still working?") must answer
/// no. A pane process killed under tmux's `remain-on-exit`, or a session whose
/// terminal emulator is slow to reap, otherwise reads as alive indefinitely.
#[cfg(target_os = "linux")]
pub fn process_is_zombie(pid: u32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return false;
    };
    // The comm field is parenthesised and may itself contain spaces and
    // parens, so the state character is the first field after the LAST ") ".
    stat.rsplit_once(") ")
        .and_then(|(_, after_comm)| after_comm.split_whitespace().next())
        .is_some_and(|state| state == "Z")
}

/// Fetch the BSD process info block backing both the start-time token and the
/// zombie check below.
#[cfg(target_os = "macos")]
fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    let buffer_size = libc::c_int::try_from(size).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();

    // SAFETY: `info` points to writable storage for exactly one
    // `proc_bsdinfo`, and `buffer_size` reports that same allocation size.
    // `proc_pidinfo` initializes the complete structure only when it returns
    // that size, which is checked before `assume_init` below.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            buffer_size,
        )
    };
    if written != buffer_size {
        return None;
    }

    // SAFETY: the exact-size return above is the API contract that the output
    // buffer contains a fully initialized `proc_bsdinfo`.
    Some(unsafe { info.assume_init() })
}

/// Read the kernel's process start timestamp with microsecond precision.
#[cfg(target_os = "macos")]
pub fn process_start_time(pid: u32) -> Option<u64> {
    let info = bsd_info(pid)?;
    info.pbi_start_tvsec
        .checked_mul(1_000_000)?
        .checked_add(info.pbi_start_tvusec)
}

/// Whether `pid` has exited but not yet been reaped by its parent.
#[cfg(target_os = "macos")]
pub fn process_is_zombie(pid: u32) -> bool {
    bsd_info(pid).is_some_and(|info| info.pbi_status == libc::SZOMB)
}

/// No trustworthy, portable start-time token is currently implemented here.
/// Destructive signaling therefore fails closed on these platforms.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn process_start_time(_pid: u32) -> Option<u64> {
    None
}

/// Without a per-platform probe, a process is never assumed to be a zombie —
/// liveness then degrades to the `kill(pid, 0)` answer, as it did everywhere
/// before this check existed.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn process_is_zombie(_pid: u32) -> bool {
    false
}

/// Verify a persisted identity against the currently running process.
pub fn verify_process_identity(identity: ProcessIdentity) -> IdentityStatus {
    if !super::is_process_alive(identity.pid) {
        return IdentityStatus::Dead;
    }

    compare_start_times(identity.start_time, process_start_time(identity.pid))
}

fn compare_start_times(recorded: Option<u64>, observed: Option<u64>) -> IdentityStatus {
    match (recorded, observed) {
        (Some(recorded), Some(observed)) if recorded == observed => IdentityStatus::VerifiedAlive,
        (Some(_), Some(_)) => IdentityStatus::Dead,
        _ => IdentityStatus::Unverifiable,
    }
}

/// Send SIGTERM only after the supplied identity verifies at call time.
///
/// `Ok(false)` means the process was already gone. Missing start-time evidence
/// is an error and never degrades to signaling the raw PID.
pub fn terminate_verified(identity: ProcessIdentity) -> Result<bool> {
    match verify_process_identity(identity) {
        IdentityStatus::VerifiedAlive => super::terminate(identity.pid)
            .with_context(|| format!("failed to terminate verified process {}", identity.pid)),
        IdentityStatus::Dead => Ok(false),
        IdentityStatus::Unverifiable => Err(UnverifiedProcessIdentity::new(identity.pid).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_start_time_is_verified() {
        assert_eq!(
            compare_start_times(Some(42), Some(42)),
            IdentityStatus::VerifiedAlive
        );
    }

    #[test]
    fn present_start_time_mismatch_is_definitive_death() {
        assert_eq!(
            compare_start_times(Some(42), Some(43)),
            IdentityStatus::Dead
        );
    }

    #[test]
    fn either_missing_start_time_is_unverifiable() {
        assert_eq!(
            compare_start_times(None, Some(42)),
            IdentityStatus::Unverifiable
        );
        assert_eq!(
            compare_start_times(Some(42), None),
            IdentityStatus::Unverifiable
        );
    }

    #[test]
    fn destructive_signal_fails_closed_without_start_time() {
        let error = terminate_verified(ProcessIdentity {
            pid: std::process::id(),
            start_time: None,
        })
        .expect_err("a live raw PID must not be signalable");

        let structured = error
            .downcast_ref::<UnverifiedProcessIdentity>()
            .expect("identity refusal must remain a structured error");
        assert_eq!(structured.pid(), std::process::id());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_process_start_time_is_stable_for_the_current_process() {
        let pid = std::process::id();
        let first = process_start_time(pid).expect("current process has kernel start metadata");
        let second = process_start_time(pid).expect("current process remains queryable");

        assert_ne!(first, 0);
        assert_eq!(first, second);
    }
}
