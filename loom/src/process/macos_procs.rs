//! A read-only census of the macOS process table, for proofs that a set of
//! processes is gone. It never signals anything.
//!
//! The table comes from one `sysctl(KERN_PROC_ALL)` read, which the kernel
//! fills under its process-list lock: pid, parent, group, uids, run state and
//! start time of every process at one instant. The session id comes from
//! `getsid` and the environment from `KERN_PROCARGS2`, per pid, after that
//! read. A failure to read one of them is never absence: a process that
//! exists but cannot be read is reported [`EnvRead::Denied`], and only one
//! confirmed dead or a zombie afterwards is [`EnvRead::Gone`]. A table that grew between the size
//! query and the read, or a row that vanished while its children remain, is
//! re-read a bounded number of times; what is still unsettled then is left
//! for the caller to treat as uncertainty.
//!
//! pid 0 (`kernel_task`) is left out: it is not a process that could carry a
//! boundary, and `getsid(0)` would answer for the caller instead.
use std::time::Duration;

/// `sizeof(struct kinfo_proc)` and the field offsets read from it, on every
/// 64-bit macOS target (arm64 and x86_64 share the layout; checked against
/// `<sys/sysctl.h>` with `offsetof`). The `libc` crate has no `kinfo_proc`
/// for Apple targets.
const KINFO_SIZE: usize = 648;
const OFF_START_SEC: usize = 0; // kp_proc.p_un.__p_starttime.tv_sec (i64)
const OFF_START_USEC: usize = 8; // kp_proc.p_un.__p_starttime.tv_usec (i32)
const OFF_STAT: usize = 36; // kp_proc.p_stat (char)
const OFF_PID: usize = 40; // kp_proc.p_pid
const OFF_UID: usize = 420; // kp_eproc.e_ucred.cr_uid
const OFF_PPID: usize = 560; // kp_eproc.e_ppid
const OFF_PGID: usize = 564; // kp_eproc.e_pgid

/// Table reads before an unsettled census is returned as is.
const ATTEMPTS: usize = 4;
const RETRY_PAUSE: Duration = Duration::from_millis(10);

/// What the census learnt of one process's environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EnvRead {
    /// The wanted entry (see [`census`]), or `None` when the process has none.
    Read(Option<String>),
    /// The process exists but its environment cannot be read; the text says why.
    Denied(String),
    /// The process exited after the table read.
    Gone,
}

/// One process of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcRecord {
    pub(crate) pid: u32,
    pub(crate) ppid: u32,
    pub(crate) pgid: u32,
    /// `None` when `getsid` failed: the process exited (`env` is `Gone`) or
    /// its session cannot be read (`env` is `Denied`).
    pub(crate) sid: Option<u32>,
    /// Start time in microseconds, as `process::process_start_time` renders it.
    pub(crate) start: u64,
    /// Effective uid.
    pub(crate) uid: u32,
    pub(crate) zombie: bool,
    pub(crate) env: EnvRead,
}

/// The process table at one instant plus what could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Census {
    pub(crate) records: Vec<ProcRecord>,
    /// Why the table itself is incomplete, or `None`.
    pub(crate) incomplete: Option<String>,
}

/// Reads the table, then every process's session and environment. For each
/// process the environment answer is the entry equal to `wanted` (a
/// `NAME=value` string) when it has one, else its first entry named `NAME`.
pub(crate) fn census(wanted: &str) -> Census {
    let name = wanted.split_once('=').map_or(wanted, |(name, _)| name);
    let mut area = vec![0u8; arg_max()];
    let mut last = None;
    for attempt in 0..ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(RETRY_PAUSE);
        }
        let census = match read_table() {
            Ok(rows) => Census {
                records: rows
                    .into_iter()
                    .map(|row| complete(row, &mut area, name, wanted))
                    .collect(),
                incomplete: None,
            },
            Err(reason) => Census {
                records: Vec::new(),
                incomplete: Some(reason),
            },
        };
        if settled(&census) {
            return census;
        }
        last = Some(census);
    }
    last.unwrap_or(Census {
        records: Vec::new(),
        incomplete: Some("the process table was not read".into()),
    })
}

/// No table error and no vanished row that still has a child in the table.
fn settled(census: &Census) -> bool {
    census.incomplete.is_none()
        && census
            .records
            .iter()
            .filter(|r| r.env == EnvRead::Gone)
            .all(|gone| census.records.iter().all(|r| r.ppid != gone.pid))
}

/// Adds the session and the environment answer to a table row. A zombie runs
/// nothing and has no environment left; it is not read.
fn complete(mut row: ProcRecord, area: &mut [u8], name: &str, wanted: &str) -> ProcRecord {
    if row.zombie {
        row.env = EnvRead::Read(None);
        return row;
    }
    let Ok(raw_pid) = libc::pid_t::try_from(row.pid) else {
        row.env = EnvRead::Denied(format!("pid {} is out of range", row.pid));
        return row;
    };
    // SAFETY: `getsid` reads one process attribute and touches no memory; the
    // pid is never 0 (`read_table` drops it), which would name the caller.
    let sid = unsafe { libc::getsid(raw_pid) };
    row.sid = u32::try_from(sid).ok();
    row.env = match row.sid {
        Some(_) => read_env(raw_pid, area, name, wanted),
        None => unread(row.pid, "its session cannot be read"),
    };
    row
}

/// The answer for a process whose session or environment read just failed:
/// [`EnvRead::Gone`] only when the process is confirmed dead or a zombie
/// (`is_process_alive` treats a zombie as dead). An error code alone is not
/// trusted: `KERN_PROCARGS2` answers `EINVAL` alike for an exited process, a
/// zombie, another user's process and a sandbox refusal, so a process that
/// still runs stays [`EnvRead::Denied`], uncertain.
fn unread(pid: u32, what: &str) -> EnvRead {
    let error = std::io::Error::last_os_error();
    if !super::is_process_alive(pid) {
        return EnvRead::Gone;
    }
    EnvRead::Denied(format!("{what}: {error}"))
}

/// One `KERN_PROC_ALL` read, sized with headroom; a table that outgrew the
/// headroom fails with `ENOMEM` and is reported for a retry.
fn read_table() -> Result<Vec<ProcRecord>, String> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_ALL];
    let mut size = 0usize;
    // SAFETY: `mib` holds 3 valid integers; a null output buffer asks only for
    // the needed size, written to `size`.
    let rc = unsafe {
        let (old, new) = (std::ptr::null_mut(), std::ptr::null_mut());
        libc::sysctl(mib.as_mut_ptr(), 3, old, &mut size, new, 0)
    };
    if rc != 0 {
        return Err(format!("cannot size the process table: {}", errno()));
    }
    let mut buffer = vec![0u8; size + size / 8 + 16 * KINFO_SIZE];
    let mut written = buffer.len();
    // SAFETY: `buffer` is writable for `written` bytes, the length passed; the
    // kernel writes at most that many and stores the count in `written`.
    let rc = unsafe {
        let new = std::ptr::null_mut();
        let old = buffer.as_mut_ptr().cast();
        libc::sysctl(mib.as_mut_ptr(), 3, old, &mut written, new, 0)
    };
    if rc != 0 {
        return Err(format!("cannot read the process table: {}", errno()));
    }
    if !written.is_multiple_of(KINFO_SIZE) || written > buffer.len() {
        return Err(format!("the process table read returned {written} bytes"));
    }
    Ok(buffer[..written]
        .as_chunks::<KINFO_SIZE>()
        .0
        .iter()
        .map(row)
        .filter(|row| row.pid != 0)
        .collect())
}

fn row(raw: &[u8; KINFO_SIZE]) -> ProcRecord {
    let u32_at = |at: usize| u32::from_ne_bytes([raw[at], raw[at + 1], raw[at + 2], raw[at + 3]]);
    let mut sec = [0u8; 8];
    sec.copy_from_slice(&raw[OFF_START_SEC..OFF_START_SEC + 8]);
    let sec = u64::try_from(i64::from_ne_bytes(sec)).unwrap_or(0);
    let usec = u64::from(u32_at(OFF_START_USEC));
    ProcRecord {
        pid: u32_at(OFF_PID),
        ppid: u32_at(OFF_PPID),
        pgid: u32_at(OFF_PGID),
        sid: None,
        start: sec.saturating_mul(1_000_000).saturating_add(usec),
        uid: u32_at(OFF_UID),
        zombie: u32::from(raw[OFF_STAT]) == libc::SZOMB,
        env: EnvRead::Gone,
    }
}

/// The environment answer for `pid` from `KERN_PROCARGS2`.
fn read_env(pid: libc::pid_t, area: &mut [u8], name: &str, wanted: &str) -> EnvRead {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut size = area.len();
    // SAFETY: `area` is writable for `size` bytes, the length passed; the
    // kernel writes at most that many and stores the count in `size`.
    let rc = unsafe {
        let new = std::ptr::null_mut();
        let old = area.as_mut_ptr().cast();
        libc::sysctl(mib.as_mut_ptr(), 3, old, &mut size, new, 0)
    };
    if rc != 0 {
        // `read_table` rows hold non-negative pids, so this cannot fail.
        let pid = u32::try_from(pid).unwrap_or(0);
        return unread(pid, "its environment cannot be read");
    }
    match marker_entry(&area[..size.min(area.len())], name, wanted) {
        Some(found) => EnvRead::Read(found),
        None => EnvRead::Denied("its argument area is malformed".into()),
    }
}

/// `kern.argmax`, the largest argument area, or 1 MiB when it cannot be read.
fn arg_max() -> usize {
    let mut mib = [libc::CTL_KERN, libc::KERN_ARGMAX];
    let mut value: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    // SAFETY: `value` is writable for `size` bytes, the length passed.
    let rc = unsafe {
        let old = (&mut value as *mut libc::c_int).cast();
        libc::sysctl(mib.as_mut_ptr(), 2, old, &mut size, std::ptr::null_mut(), 0)
    };
    match usize::try_from(value) {
        Ok(value) if rc == 0 && value > 0 => value,
        _ => 1 << 20,
    }
}

/// Parses a `KERN_PROCARGS2` area: `argc` (native `i32`), the executable
/// path, NUL padding, `argc` arguments, then the environment up to an empty
/// string or the end. `Some(found)` with the entry equal to `wanted`, else
/// the first entry named `name`, else `None`; `None` overall when the area is
/// truncated before the environment.
fn marker_entry(area: &[u8], name: &str, wanted: &str) -> Option<Option<String>> {
    let argc = i32::from_ne_bytes(area.get(..4)?.try_into().ok()?);
    let argc = usize::try_from(argc).ok()?;
    let mut rest = &area[4..];
    let path_end = rest.iter().position(|b| *b == 0)?;
    rest = &rest[path_end..];
    let start = rest.iter().position(|b| *b != 0).unwrap_or(rest.len());
    let mut strings = rest[start..].split(|b| *b == 0);
    for _ in 0..argc {
        strings.next()?;
    }
    let prefix = format!("{name}=");
    let mut first = None;
    for entry in strings.take_while(|s| !s.is_empty()) {
        if entry == wanted.as_bytes() {
            return Some(Some(wanted.to_string()));
        }
        if first.is_none() && entry.starts_with(prefix.as_bytes()) {
            first = Some(String::from_utf8_lossy(entry).into_owned());
        }
    }
    Some(first)
}

fn errno() -> std::io::Error {
    std::io::Error::last_os_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(argc: i32, strings: &[&str]) -> Vec<u8> {
        let mut area = argc.to_ne_bytes().to_vec();
        area.extend_from_slice(b"/bin/exe\0\0\0\0");
        for s in strings {
            area.extend_from_slice(s.as_bytes());
            area.push(0);
        }
        area
    }

    #[test]
    fn the_exact_marker_wins_over_an_earlier_entry_of_the_same_name() {
        let raw = area(2, &["sh", "-c", "A=1", "M=other", "M=inv", "", "M=late"]);
        assert_eq!(marker_entry(&raw, "M", "M=inv"), Some(Some("M=inv".into())));
        let raw = area(1, &["sh", "M=other", "MM=inv"]);
        assert_eq!(
            marker_entry(&raw, "M", "M=inv"),
            Some(Some("M=other".into()))
        );
    }

    #[test]
    fn arguments_and_entries_after_the_end_are_not_the_environment() {
        let raw = area(2, &["M=inv", "M=inv", "HOME=/h", "", "M=inv"]);
        assert_eq!(marker_entry(&raw, "M", "M=inv"), Some(None));
        assert_eq!(marker_entry(&area(3, &["a"]), "M", "M=inv"), None);
        assert_eq!(marker_entry(&[1, 0], "M", "M=inv"), None);
    }

    #[test]
    fn the_census_shows_this_process_with_its_identity() {
        let census = census("LOOM_TEST_NO_SUCH_MARKER=x");
        assert_eq!(census.incomplete, None, "the table read failed");
        let me = std::process::id();
        let row = census
            .records
            .iter()
            .find(|r| r.pid == me)
            .expect("the table lists the calling process");
        let raw = |pid: nix::unistd::Pid| u32::try_from(pid.as_raw()).unwrap();
        assert_eq!(row.ppid, std::os::unix::process::parent_id());
        assert_eq!(row.pgid, raw(nix::unistd::getpgrp()));
        assert_eq!(row.sid, Some(raw(nix::unistd::getsid(None).unwrap())));
        assert_eq!(Some(row.start), super::super::process_start_time(me));
        // SAFETY: `geteuid` reads the caller's credentials and cannot fail.
        assert_eq!(row.uid, unsafe { libc::geteuid() });
        assert_eq!(row.env, EnvRead::Read(None));
        assert!(census.records.iter().all(|r| r.pid != 0));
    }
}
