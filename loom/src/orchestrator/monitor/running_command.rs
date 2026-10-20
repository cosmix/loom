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

/// One row of the process table, as much of it as this module reads.
#[derive(Debug, PartialEq, Eq)]
struct ProcessRow {
    pid: u32,
    ppid: u32,
    zombie: bool,
    name: String,
}

/// The first command `session`'s agent is still running, if any.
///
/// The agent's process is the one its PID file names, through the same
/// identity check the liveness probe uses (`terminal::native::pid_guard`):
/// the wrapper `exec`s claude, so that PID is claude's own. `None` whenever
/// the evidence is missing — no PID file, a recycled PID, an unreadable
/// process table — because the watchdog this serves must keep closing silent
/// sessions when it cannot see their children.
pub(super) fn running_command(work_dir: &Path, session: &Session) -> Option<RunningCommand> {
    let (_, pid_key) = NativeBackend::window_title_and_pid_key(session)?;
    let identity = read_pid_entry(work_dir, &pid_key)?;
    if verify_process_identity(identity) == IdentityStatus::Dead {
        return None;
    }
    let rows = process_rows();
    first_command(identity.pid, &rows, process_start_time).map(|row| RunningCommand {
        pid: row.pid,
        name: row.name.clone(),
    })
}

/// The first child of `agent_pid` in `rows` that is a command by the three
/// rules in the module docs. `start_of` reads a process's start-time token.
fn first_command(
    agent_pid: u32,
    rows: &[ProcessRow],
    start_of: impl Fn(u32) -> Option<u64>,
) -> Option<&ProcessRow> {
    let agent_start = start_of(agent_pid);
    rows.iter().find(|row| {
        row.ppid == agent_pid
            && !row.zombie
            && is_command_launcher(&row.name)
            && runs_something(row.pid, rows)
            && started_after(start_of(row.pid), agent_start)
    })
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
fn is_command_launcher(name: &str) -> bool {
    COMMAND_LAUNCHERS.contains(&executable(name))
}

/// The executable a process name stands for: `ps` on macOS reports the
/// executable's path, and a login shell's name carries a leading `-`.
fn executable(name: &str) -> &str {
    let base = name.rsplit('/').next().unwrap_or(name);
    base.trim_start_matches('-')
}

#[cfg(target_os = "linux")]
fn process_rows() -> Vec<ProcessRow> {
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
fn process_rows() -> Vec<ProcessRow> {
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
