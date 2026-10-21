//! Signalling a job's process groups: retiring a child the runner holds, and
//! stopping a previous daemon's children after a restart.
//!
//! A criterion leads its own process group (`verify/criteria/confine.rs`), so
//! killing the child's group does not reach it, and on macOS `sandbox-exec`
//! is not tied to its parent. The child lists every criterion group in the
//! job's `groups` file, and every retirement kills those too.
//!
//! Retiring a job kills every listed group, even one whose leader already
//! exited: its other members are orphaned criterion processes that must die.
//! Deliberate limit: once a group's last member exits, the kernel may hand its
//! id to an unrelated group before the job is retired, which can be as late
//! as the job's deadline, and that group would be signalled. It needs a full
//! pid wrap inside one job's run; revisit if jobs ever run that long.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::{getpgrp, Pid};

use super::{store, JobRecord, JobState};
use crate::process::OwnedProcess;

const RESTART_DETAIL: &str = "daemon restarted during the run";
/// How long a previous daemon's children get to exit on SIGTERM.
const TERM_GRACE: Duration = Duration::from_secs(2);

/// Kill a held job's child (its own group, reaped within
/// `CLEANUP_GRACE`), then every criterion group the job recorded.
pub(super) fn retire(work_dir: &Path, stage_id: &str, job_id: &str, process: &mut OwnedProcess) {
    process.retire();
    kill_groups(work_dir, stage_id, job_id);
}

/// SIGKILL every criterion group the job recorded.
pub(super) fn kill_groups(work_dir: &Path, stage_id: &str, job_id: &str) {
    match store::load_groups(work_dir, stage_id, job_id) {
        Ok(groups) => {
            for pgid in groups {
                signal_group(pgid, Signal::SIGKILL);
            }
        }
        Err(error) => tracing::warn!(
            stage = %stage_id,
            job = %job_id,
            error = %format!("{error:#}"),
            "could not read a host job's process groups; its criteria may outlive it",
        ),
    }
}

/// A stage's current job a restart must stop.
enum Found {
    /// It was `Running`; its record is now `Interrupted`.
    Interrupted(JobRecord),
    /// It was cancelled after its child started, possibly while no daemon
    /// ran to retire the child.
    Cancelled(JobRecord),
}

/// Run once at daemon startup. Every `Running` record becomes `Interrupted`
/// (no daemon holds its child any more); the previous daemon's children still
/// running their job get SIGTERM, then SIGKILL after one 2 s wait; the listed
/// criterion groups whose leader still leads them get SIGKILL. A job cancelled
/// after its start whose child still runs it was never retired (a cancel
/// while no daemon ran) and is stopped the same way, its record unchanged.
/// `Queued` records stay queued. Returns the records it changed.
pub fn recover_after_restart(work_dir: &Path) -> Result<Vec<JobRecord>> {
    let (mut interrupted, mut cancelled) = (Vec::new(), Vec::new());
    for stage_id in store::stage_ids(work_dir)? {
        match interrupt(work_dir, &stage_id) {
            Ok(Some(Found::Interrupted(record))) => interrupted.push(record),
            Ok(Some(Found::Cancelled(record))) => cancelled.push(record),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                stage = %stage_id,
                error = %format!("{error:#}"),
                "could not interrupt a stage's host job after a restart",
            ),
        }
    }
    // A cancelled job a runner retired has no child left; its recorded group
    // ids may be anyone's by now, so they are left alone.
    cancelled.retain(|record| live_child(record).is_some());
    let stopping: Vec<&JobRecord> = interrupted.iter().chain(&cancelled).collect();
    stop_children(&stopping);
    for record in stopping {
        kill_led_groups(work_dir, record);
    }
    Ok(interrupted)
}

fn interrupt(work_dir: &Path, stage_id: &str) -> Result<Option<Found>> {
    let _lock = store::lock_stage(work_dir, stage_id)?;
    let Some(mut record) = store::load_current(work_dir, stage_id)? else {
        return Ok(None);
    };
    if store::cancelled_after_start(&record) {
        return Ok(Some(Found::Cancelled(record)));
    }
    if record.state != JobState::Running {
        return Ok(None);
    }
    record.state = JobState::Interrupted;
    record.detail = Some(RESTART_DETAIL.to_string());
    record.finished_at = Some(Utc::now());
    store::save_current(work_dir, &record)?;
    Ok(Some(Found::Interrupted(record)))
}

/// Stop each job's child whose command line still names the job.
fn stop_children(records: &[&JobRecord]) {
    let running: Vec<(u32, String)> = records.iter().filter_map(|r| live_child(r)).collect();
    if running.is_empty() {
        return;
    }
    for (pid, _) in &running {
        signal_group(*pid, Signal::SIGTERM);
    }
    std::thread::sleep(TERM_GRACE);
    for (pid, marker) in &running {
        if runs(*pid, marker) {
            signal_group(*pid, Signal::SIGKILL);
        }
    }
}

/// The recorded pid and its job marker, when that pid still runs the job. A
/// pid whose command line does not match is never signalled: after a restart
/// it may belong to anyone.
fn live_child(record: &JobRecord) -> Option<(u32, String)> {
    let pid = record.pid?;
    let marker = format!("verify-host {} --job {}", record.stage_id, record.id);
    runs(pid, &marker).then_some((pid, marker))
}

/// SIGKILL each listed criterion group whose leader still leads it. A group
/// whose leader is gone is left alone: its id may now be anyone's.
fn kill_led_groups(work_dir: &Path, record: &JobRecord) {
    let groups = match store::load_groups(work_dir, &record.stage_id, &record.id) {
        Ok(groups) => groups,
        Err(error) => {
            tracing::warn!(
                job = %record.id,
                error = %format!("{error:#}"),
                "could not read a recovered host job's process groups",
            );
            return;
        }
    };
    for pgid in groups {
        if ps_field("pgid", pgid).is_some_and(|leader| leader == pgid.to_string()) {
            signal_group(pgid, Signal::SIGKILL);
        }
    }
}

fn runs(pid: u32, marker: &str) -> bool {
    ps_field("command", pid).is_some_and(|command| command.contains(marker))
}

/// `ps -o <field>= -p <pid>`, trimmed; `None` when `ps` fails or finds no
/// such process.
fn ps_field(field: &str, pid: u32) -> Option<String> {
    let program = if Path::new("/bin/ps").exists() {
        "/bin/ps"
    } else {
        "ps"
    };
    let output = Command::new(program)
        .args(["-o", &format!("{field}="), "-p", &pid.to_string()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Send `signal` to the process group `pgid`, ignoring every error (`ESRCH`
/// above all). Never group 0 or 1, and never loom's own group.
fn signal_group(pgid: u32, signal: Signal) {
    let Ok(raw) = i32::try_from(pgid) else {
        return;
    };
    if raw <= 1 || raw == getpgrp().as_raw() {
        return;
    }
    let _ = killpg(Pid::from_raw(raw), signal);
}
