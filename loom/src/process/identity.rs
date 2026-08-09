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

/// Read the kernel's process start timestamp with microsecond precision.
#[cfg(target_os = "macos")]
pub fn process_start_time(pid: u32) -> Option<u64> {
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
    let info = unsafe { info.assume_init() };
    info.pbi_start_tvsec
        .checked_mul(1_000_000)?
        .checked_add(info.pbi_start_tvusec)
}

/// No trustworthy, portable start-time token is currently implemented here.
/// Destructive signaling therefore fails closed on these platforms.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn process_start_time(_pid: u32) -> Option<u64> {
    None
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
