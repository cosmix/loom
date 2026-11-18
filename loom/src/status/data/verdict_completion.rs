//! The verdict-completion state of a stage whose disputes are all ruled.
//!
//! When every dispute of a stage has a verdict that holds the stage, no judge
//! is left: the daemon runs the stage's acceptance criteria as a
//! `JobKind::VerdictComplete` host job and completes the stage itself. The
//! stage keeps `NeedsAdjudication` with no session for those minutes, so the
//! status surfaces read the job's progress to say the daemon is verifying.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::host_verify::store::{load_current, read_progress};
use crate::host_verify::{JobKind, JobState};
use crate::models::stage::{Stage, StageStatus};
use crate::utils::format_elapsed;

/// Progress of the verdict-completion job the daemon runs for a stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictCompleting {
    /// Acceptance criteria whose run has finished.
    pub done: u32,
    /// Acceptance criteria the stage declares.
    pub total: u32,
    /// The job has not started running yet.
    pub queued: bool,
    /// Seconds since the job started; `None` while it is queued.
    pub running_secs: Option<i64>,
}

impl VerdictCompleting {
    /// `disputes ruled, daemon verifying 11/14 · 3m12s`, or
    /// `disputes ruled, waiting to verify` while queued.
    pub fn text(&self) -> String {
        if self.queued {
            return "disputes ruled, waiting to verify".to_owned();
        }
        let progress = format!(
            "disputes ruled, daemon verifying {}/{}",
            self.done, self.total
        );
        match self.running_secs {
            Some(secs) => format!("{progress} · {}", format_elapsed(secs)),
            None => progress,
        }
    }
}

/// The verdict-completion state, read only for a `NeedsAdjudication` stage so
/// no other stage pays for the file reads.
pub(super) fn verdict_completing_for(
    stage: &Stage,
    work_dir: &Path,
    now: DateTime<Utc>,
) -> Option<VerdictCompleting> {
    (stage.status == StageStatus::NeedsAdjudication)
        .then(|| verdict_completing(work_dir, &stage.id, stage.acceptance.len(), now))
        .flatten()
}

/// The verdict-completion job of the stage, if one is queued or running. The
/// job is bound to the stage, not to a session. Any read failure answers
/// `None`: status never fails or logs because of this.
fn verdict_completing(
    work_dir: &Path,
    stage_id: &str,
    total: usize,
    now: DateTime<Utc>,
) -> Option<VerdictCompleting> {
    let job = load_current(work_dir, stage_id).ok()??;
    let running = match job.state {
        JobState::Queued => false,
        JobState::Running => true,
        _ => return None,
    };
    // A daemon killed mid-job leaves it `Running` for good; a child that is
    // gone ended with it.
    if running
        && job
            .pid
            .is_some_and(|pid| !crate::process::is_process_alive(pid))
    {
        return None;
    }
    if job.kind != JobKind::VerdictComplete {
        return None;
    }
    let done = read_progress(work_dir, stage_id, &job.id)
        .ok()?
        .iter()
        .filter(|outcome| outcome.label.starts_with("acceptance-"))
        .count();
    Some(VerdictCompleting {
        done: u32::try_from(done).unwrap_or(u32::MAX),
        total: u32::try_from(total).unwrap_or(u32::MAX),
        queued: !running,
        running_secs: job
            .started_at
            .filter(|_| running)
            .map(|started| (now - started).num_seconds().max(0)),
    })
}

#[cfg(test)]
#[path = "verdict_completion_tests.rs"]
mod tests;
