//! `loom hook reconcile-graph --cancel`: stop the lease holder, but only when
//! the pid the lock names is still the reconciler that recorded it.
//!
//! A pid alone is not an identity: the holder may have exited and the pid been
//! reused by an unrelated process (the daemon, `loom stage complete`). The
//! holder is signalled only when its argv holds both `hook` and
//! `reconcile-graph` AND it started no later than the lock's epoch (the
//! reconciler stamps the epoch after it starts). Unknown identity leaves the
//! lease in place.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use super::lock::{read_lock, release_lease};

/// The longest one `ps` query may take. `ps` reads the process table and
/// never blocks for long; a hung one must not hang `--cancel` with it.
const PS_TIMEOUT: Duration = Duration::from_secs(5);

/// Whole seconds of slack between a process's start (`ps` reports elapsed time
/// in whole seconds) and the epoch its reconciler stamped.
const START_SKEW_SECS: u64 = 2;

/// Stop the live holder named by `lock_path`, then record its run as finished
/// with `failures` unchanged. With no holder recorded, or a recorded holder
/// that is already dead, this does nothing: the line stays an in-progress
/// marker with a dead pid, which the next hook takes over at once instead of
/// waiting out a debounce a fresh release would start.
pub(super) fn cancel_holder(lock_path: &Path, now: u64) {
    cancel_holder_with(
        lock_path,
        now,
        crate::process::is_process_alive,
        |pid, epoch| is_reconciler(pid, epoch, now),
        crate::process::terminate,
        terminated,
    );
}

fn cancel_holder_with(
    lock_path: &Path,
    now: u64,
    alive: impl Fn(u32) -> bool,
    identity: impl Fn(u32, u64) -> Option<bool>,
    signal: impl Fn(u32) -> anyhow::Result<bool>,
    stopped: impl Fn(u32) -> bool,
) {
    let Some(holder) = read_lock(lock_path).filter(|state| state.pid != 0) else {
        return;
    };
    if !alive(holder.pid) {
        return; // The next claimant can immediately recover a dead holder.
    }
    match identity(holder.pid, holder.epoch) {
        None => return,   // A failed identity query cannot justify release.
        Some(false) => {} // A confirmed unrelated PID cannot own this lease.
        Some(true) => match signal(holder.pid) {
            Ok(false) => {} // ESRCH confirms the holder has exited.
            Ok(true) if stopped(holder.pid) => {}
            Ok(true) => return,
            Err(error) => {
                tracing::debug!(%error, pid = holder.pid, "reconcile-graph: could not signal the holder");
                return;
            }
        },
    }
    release_lease(lock_path, now, holder.pid, holder.claim);
}

/// `None` means an identity query failed; `Some(false)` proves another process
/// owns the PID or it began after the lease.
fn is_reconciler(pid: u32, epoch: u64, now: u64) -> Option<bool> {
    let argv = process_argv(pid)?;
    let names = |word: &str| argv.iter().any(|arg| arg == word);
    if !(names("hook") && names("reconcile-graph")) {
        return Some(false);
    }
    let elapsed = ps_field(pid, "etime=").as_deref().and_then(parse_etime)?;
    Some(now.saturating_sub(elapsed) <= epoch.saturating_add(START_SKEW_SECS))
}

fn terminated(pid: u32) -> bool {
    // A child can remain as a zombie until its parent reaps it. It cannot run
    // another graph pass, so that state is also safe to release.
    for _ in 0..10 {
        if !crate::process::is_process_alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    ps_field(pid, "stat=").is_some_and(|status| status.starts_with('Z'))
}

/// The process's argv: NUL-split from `/proc/<pid>/cmdline`, else the
/// whitespace-split `ps -o command=` line.
pub(super) fn process_argv(pid: u32) -> Option<Vec<String>> {
    if let Ok(raw) = fs::read(format!("/proc/{pid}/cmdline")) {
        return Some(
            raw.split(|byte| *byte == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect(),
        );
    }
    let line = ps_field(pid, "command=")?;
    Some(line.split_whitespace().map(str::to_string).collect())
}

/// One `ps -p <pid> -o <field>` value, trimmed; `None` when `ps` is missing,
/// fails, prints nothing (no such process), or outlives [`PS_TIMEOUT`].
fn ps_field(pid: u32, field: &str) -> Option<String> {
    let mut command = Command::new("ps");
    command.args(["-p", &pid.to_string(), "-o", field]);
    bounded_stdout(&mut command, PS_TIMEOUT)
}

/// The trimmed stdout of `command` when it succeeds with output within
/// `timeout`; a command still running at the deadline is killed.
fn bounded_stdout(command: &mut Command, timeout: Duration) -> Option<String> {
    command.stdin(Stdio::null());
    let output = crate::process::run_bounded_output(command, timeout, "ps").ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !text.is_empty()).then_some(text)
}

/// Parse `ps`'s `etime` (`[[dd-]hh:]mm:ss`) into seconds.
fn parse_etime(text: &str) -> Option<u64> {
    let (days, clock) = match text.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, text),
    };
    let fields: Vec<&str> = clock.split(':').collect();
    if fields.len() > 3 {
        return None;
    }
    let mut seconds = 0u64;
    for field in fields {
        seconds = seconds.checked_mul(60)?.checked_add(field.parse().ok()?)?;
    }
    days.checked_mul(86_400)?.checked_add(seconds)
}

#[cfg(test)]
mod tests {
    use super::{bounded_stdout, cancel_holder_with, parse_etime};
    use crate::commands::hook::reconcile_graph::lock::{read_lock, update_lock, LockState};
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn a_query_that_outlives_its_deadline_is_killed_and_yields_nothing() {
        let started = Instant::now();

        let text = bounded_stdout(Command::new("sleep").arg("30"), Duration::from_millis(200));

        assert_eq!(text, None);
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_query_inside_its_deadline_returns_its_trimmed_output() {
        let text = bounded_stdout(
            Command::new("echo").arg("  12:34  "),
            Duration::from_secs(10),
        );

        assert_eq!(text.as_deref(), Some("12:34"));
    }

    #[test]
    fn etime_parses_every_ps_shape() {
        assert_eq!(parse_etime("00:07"), Some(7));
        assert_eq!(parse_etime("12:34"), Some(754));
        assert_eq!(parse_etime("01:02:03"), Some(3723));
        assert_eq!(parse_etime("2-01:00:00"), Some(2 * 86_400 + 3_600));
    }

    #[test]
    fn etime_rejects_malformed_text() {
        assert_eq!(parse_etime(""), None);
        assert_eq!(parse_etime("a:b"), None);
        assert_eq!(parse_etime("1:2:3:4"), None);
    }

    #[test]
    fn unknown_identity_or_failed_signal_retains_the_lease() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("reconcile.lock");
        let state = LockState {
            epoch: 100,
            pid: 4242,
            failures: 1,
            pending: true,
            claim: 77,
        };
        update_lock(&path, |_| (Some(state), ())).unwrap();
        let signal = |_| -> anyhow::Result<bool> { panic!("unknown identity must not signal") };
        cancel_holder_with(&path, 101, |_| true, |_, _| None, signal, |_| false);
        assert_eq!(read_lock(&path), Some(state));

        cancel_holder_with(
            &path,
            101,
            |_| true,
            |_, _| Some(true),
            |_| anyhow::bail!("signal refused"),
            |_| false,
        );
        assert_eq!(read_lock(&path), Some(state));
    }
}
