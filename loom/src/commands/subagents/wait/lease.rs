use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use uuid::Uuid;

use super::model::{
    BootDeadline, LeaseOwner, TerminalOutcome, WaitIdentity, WaitLease, WAIT_SCHEMA_VERSION,
};
use crate::process::{
    process_start_time, verify_process_identity, IdentityStatus, ProcessIdentity,
};

#[path = "lease_fs.rs"]
mod lease_fs;

/// Number of seconds completed wait records are retained.
pub const RESULT_RETENTION_SECS: u64 = 86_400;

/// Supplies a stable boot identity and clocks suitable for persisted deadlines.
pub trait BootClock {
    /// Returns the operating system's identity for the current boot.
    fn boot_id(&self) -> Result<String>;

    /// Returns nanoseconds elapsed on the operating system's boot clock.
    fn monotonic_ns(&self) -> Result<u64>;

    /// Returns seconds since the Unix epoch for retention bookkeeping.
    fn unix_secs(&self) -> u64;
}

/// Boot clock backed by operating-system facilities.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBootClock;

impl BootClock for SystemBootClock {
    #[cfg(target_os = "linux")]
    fn boot_id(&self) -> Result<String> {
        let value = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .context("failed to read Linux boot ID")?;
        let value = value.trim();
        if value.is_empty() {
            bail!("Linux boot ID is empty");
        }
        Ok(value.to_owned())
    }

    #[cfg(target_os = "macos")]
    fn boot_id(&self) -> Result<String> {
        macos_boot_id()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn boot_id(&self) -> Result<String> {
        bail!("boot identity is unsupported on this operating system")
    }

    #[cfg(target_os = "linux")]
    fn monotonic_ns(&self) -> Result<u64> {
        clock_nanoseconds(libc::CLOCK_BOOTTIME)
    }

    #[cfg(target_os = "macos")]
    fn monotonic_ns(&self) -> Result<u64> {
        clock_nanoseconds(libc::CLOCK_MONOTONIC)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn monotonic_ns(&self) -> Result<u64> {
        bail!("boot clock is unsupported on this operating system")
    }

    fn unix_secs(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn clock_nanoseconds(clock: libc::clockid_t) -> Result<u64> {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` is valid writable storage for one `timespec`.
    if unsafe { libc::clock_gettime(clock, &mut value) } != 0 {
        return Err(std::io::Error::last_os_error()).context("failed to read boot clock");
    }
    let seconds = u64::try_from(value.tv_sec).context("boot clock returned negative seconds")?;
    let nanos = u64::try_from(value.tv_nsec).context("boot clock returned negative nanoseconds")?;
    seconds
        .checked_mul(1_000_000_000)
        .and_then(|base| base.checked_add(nanos))
        .ok_or_else(|| anyhow!("boot clock nanoseconds overflowed u64"))
}

#[cfg(target_os = "macos")]
fn macos_boot_id() -> Result<String> {
    let name = b"kern.bootsessionuuid\0";
    let mut size = 0usize;
    // SAFETY: the name is NUL-terminated and the null output pointer requests the required size.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error()).context("failed to size macOS boot ID");
    }
    let mut bytes = vec![0u8; size];
    // SAFETY: `bytes` provides `size` writable bytes and the name remains NUL-terminated.
    if unsafe {
        libc::sysctlbyname(
            name.as_ptr().cast(),
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error()).context("failed to read macOS boot ID");
    }
    bytes.truncate(size);
    let value = std::str::from_utf8(&bytes)?.trim_matches(['\0', ' ', '\n', '\r']);
    if value.is_empty() {
        bail!("macOS boot ID is empty");
    }
    Ok(value.to_owned())
}

/// Probes ownership of a lease without performing destructive process operations.
pub trait OwnerProbe {
    /// Returns the identity of the current process.
    fn current(&self) -> LeaseOwner;

    /// Determines whether a recorded lease owner still identifies a live process.
    fn status(&self, owner: LeaseOwner) -> IdentityStatus;
}

/// Lease owner probe backed by the process identity subsystem.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemOwnerProbe;

impl OwnerProbe for SystemOwnerProbe {
    fn current(&self) -> LeaseOwner {
        let pid = std::process::id();
        ProcessIdentity {
            pid,
            start_time: process_start_time(pid),
        }
        .into()
    }

    fn status(&self, owner: LeaseOwner) -> IdentityStatus {
        verify_process_identity(owner.into())
    }
}

/// State of a persisted deadline relative to the current boot clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlineState {
    /// Time remains before the deadline.
    Remaining(Duration),
    /// The deadline has elapsed.
    Expired,
    /// The persisted deadline belongs to a different operating-system boot.
    BootChanged,
}

/// Creates a persisted deadline after `timeout` on the current boot clock.
pub fn deadline_after(clock: &dyn BootClock, timeout: Duration) -> Result<BootDeadline> {
    let timeout_ns = u64::try_from(timeout.as_nanos()).context("wait timeout is too large")?;
    let boot_id = clock.boot_id()?;
    let monotonic_ns = clock
        .monotonic_ns()?
        .checked_add(timeout_ns)
        .ok_or_else(|| anyhow!("wait deadline overflowed u64"))?;
    Ok(BootDeadline {
        boot_id,
        monotonic_ns,
    })
}

/// Compares a persisted deadline with the supplied boot clock.
pub fn deadline_state(deadline: &BootDeadline, clock: &dyn BootClock) -> Result<DeadlineState> {
    if clock.boot_id()? != deadline.boot_id {
        return Ok(DeadlineState::BootChanged);
    }
    let now = clock.monotonic_ns()?;
    if now >= deadline.monotonic_ns {
        Ok(DeadlineState::Expired)
    } else {
        Ok(DeadlineState::Remaining(Duration::from_nanos(
            deadline.monotonic_ns - now,
        )))
    }
}

/// Filesystem location holding the active lease and retained results for one parent session.
#[derive(Debug, Clone)]
pub struct LeaseDir {
    root: PathBuf,
    dirfd: Arc<OwnedFd>,
}

impl PartialEq for LeaseDir {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root
    }
}

impl Eq for LeaseDir {}

impl LeaseDir {
    /// Opens or creates the safe lease directory derived from `identity`.
    pub fn open(scratch_root: &Path, identity: &WaitIdentity) -> Result<Self> {
        lease_fs::open_lease_dir(scratch_root, identity).map(|(root, dirfd)| Self {
            root,
            dirfd: Arc::new(dirfd),
        })
    }

    /// Returns the parent-session lease directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn dirfd(&self) -> RawFd {
        self.dirfd.as_ref().as_raw_fd()
    }
}

/// Result of attempting to acquire the parent-session wait lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acquired {
    /// The caller owns the newly written lease.
    Owner(WaitLease),
    /// An active lease already waits for the same exact workers.
    AlreadyWaiting(WaitLease),
    /// An active lease waits for a different worker set.
    Busy(WaitLease),
}

/// Acquires the parent-session lease or reports the existing active wait.
pub fn acquire(
    dir: &LeaseDir,
    identity: &WaitIdentity,
    timeout: Duration,
    source_revision: &str,
    clock: &dyn BootClock,
    probe: &dyn OwnerProbe,
) -> Result<Acquired> {
    let _lock = lease_fs::lock(dir.dirfd(), dir.root())?;
    let existing = lease_fs::read_active(dir.dirfd(), dir.root())?;
    if let Some(ref lease) = existing {
        if lease.terminal_result.is_none() {
            match stale_outcome(lease, clock, probe)? {
                Some(outcome) => lease_fs::write_result(
                    dir.dirfd(),
                    dir.root(),
                    lease,
                    outcome,
                    clock.unix_secs(),
                )?,
                None if lease.identity.worker_specs() == identity.worker_specs() => {
                    return Ok(Acquired::AlreadyWaiting(lease.clone()));
                }
                None => return Ok(Acquired::Busy(lease.clone())),
            }
        }
    }
    let lease = new_lease(identity, timeout, source_revision, clock, probe)?;
    lease_fs::write_active(dir.dirfd(), dir.root(), &lease)?;
    Ok(Acquired::Owner(lease))
}

fn stale_outcome(
    lease: &WaitLease,
    clock: &dyn BootClock,
    probe: &dyn OwnerProbe,
) -> Result<Option<TerminalOutcome>> {
    match deadline_state(&lease.deadline, clock)? {
        DeadlineState::BootChanged => Ok(Some(TerminalOutcome::Interrupted)),
        DeadlineState::Expired => Ok(Some(TerminalOutcome::TimedOut)),
        DeadlineState::Remaining(_) => match probe.status(lease.owner) {
            IdentityStatus::Dead => Ok(Some(TerminalOutcome::Interrupted)),
            IdentityStatus::VerifiedAlive | IdentityStatus::Unverifiable => Ok(None),
        },
    }
}

fn new_lease(
    identity: &WaitIdentity,
    timeout: Duration,
    source_revision: &str,
    clock: &dyn BootClock,
    probe: &dyn OwnerProbe,
) -> Result<WaitLease> {
    Ok(WaitLease {
        schema_version: WAIT_SCHEMA_VERSION,
        wait_id: Uuid::new_v4().to_string(),
        canonical_repo: identity.canonical_repo.clone(),
        source_revision: source_revision.to_owned(),
        identity: identity.clone(),
        deadline: deadline_after(clock, timeout)?,
        owner: probe.current(),
        terminal_result: None,
        finished_unix_secs: None,
    })
}

/// Persists a terminal result and releases the matching active lease.
pub fn finish(
    dir: &LeaseDir,
    lease: &WaitLease,
    outcome: TerminalOutcome,
    clock: &dyn BootClock,
) -> Result<()> {
    let _lock = lease_fs::lock(dir.dirfd(), dir.root())?;
    lease_fs::write_result(dir.dirfd(), dir.root(), lease, outcome, clock.unix_secs())?;
    if let Some(active) = lease_fs::read_active(dir.dirfd(), dir.root())? {
        if active.wait_id == lease.wait_id && active.owner == lease.owner {
            lease_fs::remove_active(dir.dirfd(), dir.root())?;
        }
    }
    Ok(())
}

/// Removes valid terminal result records older than the retention window.
pub fn prune_results(dir: &LeaseDir, clock: &dyn BootClock) -> Result<usize> {
    let _lock = lease_fs::lock(dir.dirfd(), dir.root())?;
    lease_fs::prune(
        dir.dirfd(),
        dir.root(),
        clock.unix_secs(),
        RESULT_RETENTION_SECS,
    )
}
