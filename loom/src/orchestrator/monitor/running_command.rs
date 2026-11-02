//! Whether a session's agent is waiting on a command it started.
//!
//! Claude Code runs every Bash tool call as a direct child of its own
//! process: a shell (`bash`/`zsh`, or `sh` for a hook), or the sandbox
//! launcher that wraps one (`bwrap` on Linux, `sandbox-exec` on macOS). A call
//! sent to the background keeps that child alive after the turn ends, and that
//! child is the only trace on the host that the agent is still waiting for
//! work it started.
//!
//! Every other child the agent keeps says nothing about work, because it is
//! there for the whole session: the wrapper's stderr `tee`, the sandbox's
//! `socat` proxy bridge, MCP and language servers. A launcher child counts as
//! a command only when all three hold:
//!
//! - it is named like a launcher (above);
//! - it has a live child of its own that is not `tee`. A tool call's shell
//!   runs something; the session wrapper (`terminal::native::wrapper`) starts
//!   its `tee` through a process substitution forked before `exec`, which can
//!   leave an idle `bash` subshell whose only child is that `tee`;
//! - it started later than the agent's process, by the kernel's start-time
//!   token ([`process_start_time`]: clock ticks on Linux, 10 ms at the usual
//!   100 Hz, microseconds on macOS). Anything the wrapper forked before
//!   `exec` shares the agent's start or follows it within a tick; a real tool
//!   call starts seconds later. When either start time is unreadable this
//!   test is skipped and the other two decide.
//!
//! Known limit: a server launched through a shell that stays its parent (an
//! MCP server configured as `sh -c 'node server.js; ...'`) reads as a command
//! for as long as it runs. The judge watchdog bounds what that costs with its
//! hold ceiling (`super::judge_watchdog`). Revisit if a session is observed
//! held by one.

use std::path::Path;

use crate::models::session::Session;
use crate::orchestrator::terminal::native::{read_pid_entry, NativeBackend};
#[cfg(not(target_os = "linux"))]
use crate::process::run_bounded_output;
use crate::process::{process_start_time, verify_process_identity, IdentityStatus};

/// Executable names a Bash tool call runs under.
const COMMAND_LAUNCHERS: &[&str] = &["sh", "bash", "zsh", "bwrap", "sandbox-exec"];

/// The wrapper's stderr capture, the one child a launcher may keep without
/// running anything.
const WRAPPER_TEE: &str = "tee";

/// How long the `ps` listing may take where there is no `/proc` to read.
#[cfg(not(target_os = "linux"))]
const PS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// A command a session's agent started and is still running.
#[derive(Debug)]
pub(super) struct RunningCommand {
    pub(super) pid: u32,
    pub(super) name: String,
}

/// CPU ticks in the live process trees of `commands`. A positive delta between
/// polls is observed work; merely retaining an open shell is not.
pub(super) fn command_cpu_ticks(commands: &[u32]) -> Option<u64> {
    tree_cpu_ticks(commands, &process_rows(), process_cpu_ticks)
}

/// Ticks under `roots`. Every root must be live; a descendant that exits
/// between the listing and its read contributes nothing.
pub(super) fn tree_cpu_ticks(
    roots: &[u32],
    rows: &[ProcessRow],
    ticks_of: impl Fn(u32) -> Option<u64>,
) -> Option<u64> {
    let mut total = 0u64;
    for root in roots {
        rows.iter().find(|row| row.pid == *root && !row.zombie)?;
        total = total.checked_add(ticks_of(*root)?)?;
    }
    let mut pending = roots.to_vec();
    let mut seen = roots.to_vec();
    while let Some(pid) = pending.pop() {
        for child in rows.iter().filter(|row| row.ppid == pid && !row.zombie) {
            if !seen.contains(&child.pid) {
                seen.push(child.pid);
                pending.push(child.pid);
                total = total.checked_add(ticks_of(child.pid).unwrap_or(0))?;
            }
        }
    }
    Some(total)
}

#[cfg(target_os = "linux")]
pub(super) fn process_cpu_ticks(pid: u32) -> Option<u64> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    parse_cpu_ticks(&stat)
}

#[cfg(target_os = "macos")]
pub(super) fn process_cpu_ticks(pid: u32) -> Option<u64> {
    let mut info = std::mem::MaybeUninit::<libc::proc_taskinfo>::uninit();
    let size = i32::try_from(std::mem::size_of::<libc::proc_taskinfo>()).ok()?;
    // SAFETY: the kernel receives exactly the writable proc_taskinfo buffer
    // size. Only its exact-size success permits reading initialized fields.
    let written = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDTASKINFO,
            0,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // SAFETY: proc_pidinfo initialized all bytes on the checked success path.
    let info = unsafe { info.assume_init() };
    info.pti_total_user.checked_add(info.pti_total_system)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn process_cpu_ticks(pid: u32) -> Option<u64> {
    let mut ps = std::process::Command::new("ps");
    ps.args(["-o", "time=", "-p", &pid.to_string()]);
    let output = run_bounded_output(&mut ps, PS_TIMEOUT, "process CPU time").ok()?;
    if !output.status.success() {
        return None;
    }
    let text = std::str::from_utf8(&output.stdout).ok()?;
    parse_ps_cpu_time(text.trim())
}

#[cfg(any(not(any(target_os = "linux", target_os = "macos")), test))]
fn parse_ps_cpu_time(text: &str) -> Option<u64> {
    let (days, clock) = text.split_once('-').map_or((0, text), |(days, clock)| {
        (days.parse::<u64>().ok().unwrap_or(u64::MAX), clock)
    });
    let parts: Vec<_> = clock.split(':').collect();
    let seconds = match parts.as_slice() {
        [minutes, seconds] => minutes
            .parse::<u64>()
            .ok()?
            .checked_mul(60)?
            .checked_add(seconds.split('.').next()?.parse::<u64>().ok()?)?,
        [hours, minutes, seconds] => hours
            .parse::<u64>()
            .ok()?
            .checked_mul(3600)?
            .checked_add(minutes.parse::<u64>().ok()?.checked_mul(60)?)?
            .checked_add(seconds.split('.').next()?.parse::<u64>().ok()?)?,
        _ => return None,
    };
    days.checked_mul(86_400)?.checked_add(seconds)
}

/// A process's own ticks plus those of the children it has reaped, so a tool
/// that finishes leaves its work in the total instead of shrinking it. The
/// macOS task counters have no reaped-children figure.
#[cfg(any(target_os = "linux", test))]
fn parse_cpu_ticks(stat: &str) -> Option<u64> {
    let (_, fields) = stat.rsplit_once(") ")?;
    let fields: Vec<_> = fields.split_whitespace().collect();
    fields[..]
        .get(11..15)?
        .iter()
        .try_fold(0u64, |total, field| {
            total.checked_add(field.parse::<u64>().ok()?)
        })
}

/// The kernel-visible argv of `pid`, separators and all.
#[cfg(target_os = "linux")]
pub(super) fn process_argv(pid: u32) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(String::from_utf8_lossy(&raw).into_owned())
}

#[cfg(not(target_os = "linux"))]
pub(super) fn process_argv(pid: u32) -> Option<String> {
    let mut ps = std::process::Command::new("ps");
    ps.args(["-ww", "-o", "command=", "-p", &pid.to_string()]);
    let output = run_bounded_output(&mut ps, PS_TIMEOUT, "process command line").ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// One row of the process table, as much of it as this module reads.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ProcessRow {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    pub(super) zombie: bool,
    pub(super) name: String,
}

/// The oldest command `session`'s agent is still running, if any.
pub(super) fn running_command(work_dir: &Path, session: &Session) -> Option<RunningCommand> {
    running_commands(work_dir, session).into_iter().next()
}

/// Every command `session`'s agent is still running, oldest first.
///
/// The agent's process is the one its PID file names, through the same
/// identity check the liveness probe uses (`terminal::native::pid_guard`):
/// the wrapper `exec`s claude, so that PID is claude's own. Empty whenever
/// the evidence is missing — no PID file, a recycled PID, an unreadable
/// process table — because the watchdog this serves must keep closing silent
/// sessions when it cannot see their children.
pub(super) fn running_commands(work_dir: &Path, session: &Session) -> Vec<RunningCommand> {
    let Some(agent) = verified_agent(work_dir, session) else {
        return Vec::new();
    };
    let rows = process_rows();
    agent_commands(agent, &rows, process_start_time)
        .into_iter()
        .map(|row| RunningCommand {
            pid: row.pid,
            name: row.name.clone(),
        })
        .collect()
}

fn verified_agent(work_dir: &Path, session: &Session) -> Option<u32> {
    let (_, pid_key) = NativeBackend::window_title_and_pid_key(session)?;
    let identity = read_pid_entry(work_dir, &pid_key)?;
    (verify_process_identity(identity) == IdentityStatus::VerifiedAlive).then_some(identity.pid)
}

/// The children of `agent_pid` in `rows` that are commands by the three rules in
/// the module docs, oldest first, so the head stays the same one while the
/// agent starts more beside it. `start_of` reads a process's start-time token;
/// a start it cannot read sorts last, and a shared start falls back to the PID.
fn agent_commands(
    agent_pid: u32,
    rows: &[ProcessRow],
    start_of: impl Fn(u32) -> Option<u64>,
) -> Vec<&ProcessRow> {
    let agent_start = start_of(agent_pid);
    let mut commands: Vec<(u64, &ProcessRow)> = rows
        .iter()
        .filter(|row| {
            row.ppid == agent_pid
                && !row.zombie
                && is_command_launcher(&row.name)
                && runs_something(row.pid, rows)
        })
        .filter_map(|row| {
            let start = start_of(row.pid);
            started_after(start, agent_start).then_some((start.unwrap_or(u64::MAX), row))
        })
        .collect();
    commands.sort_by_key(|(start, row)| (*start, row.pid));
    commands.into_iter().map(|(_, row)| row).collect()
}

/// Whether `pid` has a live child that is not the wrapper's `tee`.
fn runs_something(pid: u32, rows: &[ProcessRow]) -> bool {
    rows.iter()
        .any(|row| row.ppid == pid && !row.zombie && executable(&row.name) != WRAPPER_TEE)
}

/// Whether a child started strictly after the agent; `true` when either
/// start time is unknown, so the other rules decide alone.
fn started_after(child_start: Option<u64>, agent_start: Option<u64>) -> bool {
    match (child_start, agent_start) {
        (Some(child), Some(agent)) => child > agent,
        _ => true,
    }
}

/// Whether a process name is one a Bash tool call runs under.
pub(super) fn is_command_launcher(name: &str) -> bool {
    COMMAND_LAUNCHERS.contains(&executable(name))
}

/// The executable a process name stands for: `ps` on macOS reports the
/// executable's path, and a login shell's name carries a leading `-`.
fn executable(name: &str) -> &str {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.trim_start_matches('-')
}

#[cfg(target_os = "linux")]
pub(super) fn process_rows() -> Vec<ProcessRow> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            parse_proc_stat(pid, &stat)
        })
        .collect()
}

/// One `/proc/<pid>/stat` line. The name is parenthesised and may itself
/// contain spaces and parens, so it ends at the LAST ") " — the rule
/// `process::identity` reads the start time by. After it: state, ppid.
#[cfg(any(target_os = "linux", test))]
fn parse_proc_stat(pid: u32, stat: &str) -> Option<ProcessRow> {
    let (head, after_name) = stat.rsplit_once(") ")?;
    let (_, name) = head.split_once(" (")?;
    let mut fields = after_name.split_whitespace();
    let state = fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    Some(ProcessRow {
        pid,
        ppid,
        zombie: state == "Z",
        name: name.to_string(),
    })
}

/// No `/proc`: `ps` is POSIX, and the same fallback
/// `daemon::server::peer_identity` uses on macOS.
#[cfg(not(target_os = "linux"))]
pub(super) fn process_rows() -> Vec<ProcessRow> {
    let mut ps = std::process::Command::new("ps");
    ps.args(["-A", "-o", "pid=,ppid=,stat=,comm="]);
    let output = match run_bounded_output(&mut ps, PS_TIMEOUT, "process table listing") {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    let listing = String::from_utf8_lossy(&output.stdout);
    listing.lines().filter_map(parse_ps_row).collect()
}

/// One `ps -o pid=,ppid=,stat=,comm=` line. The name is the rest of the line:
/// macOS reports the executable's path there, which may contain spaces.
#[cfg(any(not(target_os = "linux"), test))]
fn parse_ps_row(line: &str) -> Option<ProcessRow> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse().ok()?;
    let ppid = fields.next()?.parse().ok()?;
    let state = fields.next()?;
    let name = fields.collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return None;
    }
    Some(ProcessRow {
        pid,
        ppid,
        zombie: state.starts_with('Z'),
        name,
    })
}

#[cfg(test)]
#[path = "running_command_tests.rs"]
mod tests;
