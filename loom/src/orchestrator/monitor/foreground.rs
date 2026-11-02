//! Bounded foreground work evidence for stage stall decisions.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::models::session::{Session, SessionType};

use super::detection::Detection;
use super::direct_worker::argv_names_invocation;
use super::foreground_progress::{record, recorded, within_window};
pub(super) use super::foreground_progress::{CommandProgress, ForegroundObservation};
use super::heartbeat::Heartbeat;
use super::running_command::{command_cpu_ticks, process_argv, running_command, running_commands};

/// A current foreground forward, with its process identity rechecked.
pub(crate) fn foreground_forward_holds(
    work_dir: &Path,
    session: &Session,
    heartbeat: Option<&Heartbeat>,
    timeout_secs: u64,
    now: DateTime<Utc>,
) -> bool {
    if session.session_type != SessionType::Stage || timeout_secs == 0 {
        return false;
    }
    let Some(command) = running_command(work_dir, session) else {
        return false;
    };
    let Some(started) = crate::process::process_start_time(command.pid) else {
        return false;
    };
    let Some(stage_id) = session.stage_id.as_deref() else {
        return false;
    };
    let active = crate::codex_lifecycle::foreground_forward_active(
        work_dir,
        stage_id,
        &session.id,
        baseline(session, heartbeat),
        Duration::from_secs(timeout_secs),
        now,
    ) == Some(true);
    active && crate::process::process_start_time(command.pid) == Some(started)
}

/// Both status and the monitor sample the same protected checkpoint.
pub(crate) fn foreground_command_active(
    work_dir: &Path,
    session: &Session,
    heartbeat: Option<&Heartbeat>,
    timeout_secs: u64,
    now: DateTime<Utc>,
) -> bool {
    command_observation(work_dir, session, heartbeat, timeout_secs, now).is_some_and(
        |observation| observation.holds(baseline(session, heartbeat), timeout_secs, now),
    )
}

fn baseline(session: &Session, heartbeat: Option<&Heartbeat>) -> DateTime<Utc> {
    heartbeat
        .filter(|heartbeat| heartbeat.session_id == session.id)
        .map_or(session.created_at, Heartbeat::effective_progress_at)
}

fn command_observation(
    work_dir: &Path,
    session: &Session,
    heartbeat: Option<&Heartbeat>,
    timeout_secs: u64,
    now: DateTime<Utc>,
) -> Option<ForegroundObservation> {
    let baseline = baseline(session, heartbeat);
    if session.session_type != SessionType::Stage
        || session.status.is_terminal()
        || !within_window(baseline, timeout_secs, now)
    {
        return None;
    }
    // A forward with fresh evidence holds the stage itself. One without leaves a
    // command running beside it free to earn credit.
    let forward_fresh = session.stage_id.as_deref().is_some_and(|stage| {
        crate::codex_lifecycle::foreground_forward_active(
            work_dir,
            stage,
            &session.id,
            baseline,
            Duration::from_secs(timeout_secs),
            now,
        ) == Some(true)
    });
    if forward_fresh {
        return None;
    }
    let commands = creditable_commands(work_dir, session);
    observe_commands(
        work_dir,
        &session.id,
        &commands,
        baseline,
        timeout_secs,
        now,
    )
}

/// What can earn CPU credit: every running command, oldest first, except the
/// launch of an authorized forward, whose supervisor polls whether or not its
/// worker does anything.
fn creditable_commands(work_dir: &Path, session: &Session) -> Vec<u32> {
    let invocations = session.stage_id.as_deref().map_or_else(Vec::new, |stage| {
        crate::codex_lifecycle::session_invocation_ids(work_dir, stage, &session.id)
    });
    let is_forward = |pid: u32| {
        !invocations.is_empty()
            && process_argv(pid).is_some_and(|argv| {
                invocations
                    .iter()
                    .any(|invocation| argv_names_invocation(&argv, invocation))
            })
    };
    running_commands(work_dir, session)
        .into_iter()
        .map(|command| command.pid)
        .filter(|pid| !is_forward(*pid))
        .collect()
}

/// Sample `commands` and record the sum under the oldest one, the identity that
/// stays the same while the agent starts and ends others beside it. A command
/// that exits between being found and being sampled leaves no sample, and that
/// says nothing about the work seen so far: what is on record stands, and its
/// idle grace still runs out. When the oldest ends, the next one starts a new
/// observation with the first-sight grace any new command gets, bounded by the
/// wall window.
pub(super) fn observe_commands(
    work_dir: &Path,
    session_id: &str,
    commands: &[u32],
    baseline: DateTime<Utc>,
    timeout_secs: u64,
    now: DateTime<Utc>,
) -> Option<ForegroundObservation> {
    let pid = *commands.first()?;
    let sampled = crate::process::process_start_time(pid).and_then(|started| {
        let progress = CommandProgress {
            pid,
            started: Some(started),
            cpu_ticks: command_cpu_ticks(commands)?,
        };
        record(work_dir, session_id, progress, baseline, timeout_secs, now).ok()
    });
    sampled.or_else(|| recorded(work_dir, session_id, pid))
}

impl Detection {
    pub(crate) fn foreground_command_holds(
        &mut self,
        work_dir: &Path,
        session: &Session,
        heartbeat: Option<&Heartbeat>,
        timeout_secs: u64,
        now: DateTime<Utc>,
    ) -> bool {
        let Some(observation) =
            command_observation(work_dir, session, heartbeat, timeout_secs, now)
        else {
            return false;
        };
        let holds = observation.holds(baseline(session, heartbeat), timeout_secs, now);
        self.foreground_commands_seen
            .insert(session.id.clone(), observation);
        holds
    }

    #[cfg(test)]
    pub(super) fn record_command_progress(
        &mut self,
        session_id: &str,
        progress: CommandProgress,
        baseline: DateTime<Utc>,
        timeout_secs: u64,
        now: DateTime<Utc>,
    ) -> bool {
        let previous = self.foreground_commands_seen.remove(session_id);
        let Some(observation) =
            ForegroundObservation::advance(previous, progress, baseline, timeout_secs, now)
        else {
            return false;
        };
        let holds = observation.holds(baseline, timeout_secs, now);
        self.foreground_commands_seen
            .insert(session_id.to_owned(), observation);
        holds
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "foreground_parallel_tests.rs"]
mod parallel_tests;
