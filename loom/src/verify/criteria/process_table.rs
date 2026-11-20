//! A snapshot of the host's process table: each process's parent, process
//! group and session. The numbers name no process by themselves; a caller
//! confirms a start time before it signals a pid.

/// One process as the table showed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProcessEntry {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    pub(super) pgid: u32,
    pub(super) sid: u32,
}

/// Every process this process can see. A process that exits while the table
/// is read is left out.
#[cfg(target_os = "macos")]
pub(super) fn snapshot() -> Vec<ProcessEntry> {
    all_pids().into_iter().filter_map(entry).collect()
}

/// Every process this process can see, from `/proc`.
#[cfg(target_os = "linux")]
pub(super) fn snapshot() -> Vec<ProcessEntry> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(entry)
        .collect()
}

/// No process table is read on other platforms: the caller's process-group
/// kill is all a timeout does there.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn snapshot() -> Vec<ProcessEntry> {
    Vec::new()
}

/// The table's entry for `pid`, read now.
#[cfg(target_os = "linux")]
pub(super) fn entry(pid: u32) -> Option<ProcessEntry> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_stat(pid, &stat)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn entry(_pid: u32) -> Option<ProcessEntry> {
    None
}

/// The parent, group and session fields of a `/proc/<pid>/stat` line. The
/// command name is parenthesised and may hold spaces and parentheses, so the
/// fields start after the last `)`.
#[cfg(any(target_os = "linux", test))]
fn parse_stat(pid: u32, stat: &str) -> Option<ProcessEntry> {
    let mut fields = stat.rsplit_once(')')?.1.split_whitespace().skip(1);
    let mut next = || fields.next()?.parse::<u32>().ok();
    let (ppid, pgid, sid) = (next()?, next()?, next()?);
    Some(ProcessEntry {
        pid,
        ppid,
        pgid,
        sid,
    })
}

#[cfg(target_os = "macos")]
fn all_pids() -> Vec<u32> {
    // SAFETY: a null buffer of size zero asks only for the number of pids.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let Ok(count) = usize::try_from(count) else {
        return Vec::new();
    };
    // Room for processes started between the two calls.
    let mut pids: Vec<libc::pid_t> = vec![0; count + 64];
    let bytes = pids.len() * std::mem::size_of::<libc::pid_t>();
    let bytes = libc::c_int::try_from(bytes).unwrap_or(libc::c_int::MAX);
    // SAFETY: `pids` is writable for `bytes` bytes, the size passed.
    let written = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    let Ok(written) = usize::try_from(written) else {
        return Vec::new();
    };
    pids.truncate(written.min(pids.len()));
    pids.into_iter()
        .filter_map(|pid| u32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
        .collect()
}

/// The table's entry for `pid`, read now.
#[cfg(target_os = "macos")]
pub(super) fn entry(pid: u32) -> Option<ProcessEntry> {
    let raw = libc::pid_t::try_from(pid).ok()?;
    let size = std::mem::size_of::<libc::proc_bsdshortinfo>();
    let buffer_size = libc::c_int::try_from(size).ok()?;
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdshortinfo>::uninit();
    // SAFETY: `info` is writable storage for one `proc_bsdshortinfo` and
    // `buffer_size` is its size; the structure is read only when the call
    // reports that it wrote all of it.
    let written = unsafe {
        libc::proc_pidinfo(
            raw,
            libc::PROC_PIDT_SHORTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            buffer_size,
        )
    };
    if written != buffer_size {
        return None;
    }
    // SAFETY: the exact-size return above means the structure is initialized.
    let info = unsafe { info.assume_init() };
    // SAFETY: `getsid` reads a process attribute and touches no memory.
    let sid = u32::try_from(unsafe { libc::getsid(raw) }).ok()?;
    Some(ProcessEntry {
        pid,
        ppid: info.pbsi_ppid,
        pgid: info.pbsi_pgid,
        sid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stat_line_is_read_after_the_last_parenthesis() {
        let stat = "42 (a) b (c)) S 7 42 40 0 -1 4194560 100";
        assert_eq!(
            parse_stat(42, stat),
            Some(ProcessEntry {
                pid: 42,
                ppid: 7,
                pgid: 42,
                sid: 40,
            })
        );
        assert_eq!(parse_stat(42, "42 (truncated"), None);
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn the_snapshot_shows_this_process_with_its_group_and_session() {
        let me = std::process::id();
        let entry = snapshot()
            .into_iter()
            .find(|entry| entry.pid == me)
            .expect("the table shows the calling process");
        let raw = |pid: nix::unistd::Pid| u32::try_from(pid.as_raw()).unwrap();
        assert_eq!(entry.ppid, std::os::unix::process::parent_id());
        assert_eq!(entry.pgid, raw(nix::unistd::getpgrp()));
        assert_eq!(entry.sid, raw(nix::unistd::getsid(None).unwrap()));
        assert_eq!(super::entry(me), Some(entry));
    }
}
