//! Bounded foreground work evidence for stage stall decisions.

use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::models::session::{Session, SessionType};

use super::detection::Detection;
use super::heartbeat::Heartbeat;
use super::running_command::{command_cpu_ticks, running_command};

/// One command identity and its last measured CPU progress.
pub(super) struct ForegroundObservation {
    pid: u32,
    started: Option<u64>,
    cpu_ticks: u64,
    progress_at: DateTime<Utc>,
}

pub(super) struct CommandProgress {
    pub(super) pid: u32,
    pub(super) started: Option<u64>,
    pub(super) cpu_ticks: u64,
}

const FOREGROUND_HOLD_CEILING_SECS: i64 = 1560;

/// A current foreground Codex forward with a live command and bounded job
/// evidence. The authorization must follow this attempt's last progress.
pub(crate) fn foreground_forward_holds(
    work_dir: &Path,
    session: &Session,
    heartbeat: Option<&Heartbeat>,
    timeout_secs: u64,
    now: DateTime<Utc>,
) -> bool {
    if session.session_type != SessionType::Stage
        || timeout_secs == 0
        || !running_command(work_dir, session)
            .is_some_and(|command| crate::process::process_start_time(command.pid).is_some())
    {
        return false;
    }
    let Some(stage_id) = session.stage_id.as_deref() else {
        return false;
    };
    let since = heartbeat
        .filter(|heartbeat| heartbeat.session_id == session.id)
        .map_or(session.created_at, |heartbeat| {
            heartbeat.effective_progress_at()
        });
    crate::codex_lifecycle::foreground_forward_active(
        work_dir,
        stage_id,
        &session.id,
        since,
        Duration::from_secs(timeout_secs),
        now,
    ) == Some(true)
}

impl Detection {
    /// A live command earns only one budget since measured CPU growth, and
    /// no longer than the wrapper's absolute deadline since last heartbeat.
    pub(super) fn foreground_command_holds(
        &mut self,
        work_dir: &Path,
        session: &Session,
        heartbeat: Option<&Heartbeat>,
        timeout_secs: u64,
        now: DateTime<Utc>,
    ) -> bool {
        if session.session_type != SessionType::Stage || timeout_secs == 0 {
            return false;
        }
        let baseline = heartbeat
            .filter(|heartbeat| heartbeat.session_id == session.id)
            .map_or(session.created_at, |heartbeat| {
                heartbeat.effective_progress_at()
            });
        let elapsed = now.signed_duration_since(baseline).num_seconds();
        if !(0..=FOREGROUND_HOLD_CEILING_SECS).contains(&elapsed) {
            return false;
        }
        let Some(command) = running_command(work_dir, session) else {
            self.foreground_commands_seen.remove(&session.id);
            return false;
        };
        let Some(cpu_ticks) = command_cpu_ticks(command.pid) else {
            return false;
        };
        let Some(started) = crate::process::process_start_time(command.pid) else {
            return false;
        };
        self.record_command_progress(
            &session.id,
            CommandProgress {
                pid: command.pid,
                started: Some(started),
                cpu_ticks,
            },
            baseline,
            timeout_secs,
            now,
        )
    }

    pub(super) fn record_command_progress(
        &mut self,
        session_id: &str,
        progress: CommandProgress,
        baseline: DateTime<Utc>,
        timeout_secs: u64,
        now: DateTime<Utc>,
    ) -> bool {
        let Ok(timeout) = i64::try_from(timeout_secs) else {
            return false;
        };
        let Some(duration) = chrono::Duration::try_seconds(timeout) else {
            return false;
        };
        let Some(first_at) = baseline.checked_add_signed(duration) else {
            return false;
        };
        let observation = self
            .foreground_commands_seen
            .entry(session_id.to_owned())
            .or_insert(ForegroundObservation {
                pid: progress.pid,
                started: progress.started,
                cpu_ticks: progress.cpu_ticks,
                progress_at: now.min(first_at),
            });
        if observation.pid != progress.pid || observation.started != progress.started {
            *observation = ForegroundObservation {
                pid: progress.pid,
                started: progress.started,
                cpu_ticks: progress.cpu_ticks,
                progress_at: now.min(first_at),
            };
        } else if progress.cpu_ticks > observation.cpu_ticks {
            observation.cpu_ticks = progress.cpu_ticks;
            observation.progress_at = now;
        }
        now.signed_duration_since(observation.progress_at)
            .num_seconds()
            < timeout
    }
}
