//! Native Codex jobs in the tokens pane: which job an agent row belongs to, how a job's
//! outcome and duration read, and the jobs that never got an agent row.

use chrono::{DateTime, Utc};
use ratatui::style::{Color, Style};

use crate::commands::common::model_display::model_with_effort;
use crate::commands::status::data::{CodexJob, StageSummary};
use crate::native_worker::model::{Outcome, Phase};
use crate::tokens::{AgentFlow, AgentKind, StageFlow};

/// Prefix of a codex agent id: `codex-<thread id>`, cut to the first characters of the
/// thread unless two agents of the stage would share the short form.
const CODEX_ID_PREFIX: &str = "codex-";

/// The stage's native Codex jobs, oldest first.
pub(super) fn jobs_of<'a>(stages: &'a [StageSummary], stage_id: &str) -> &'a [CodexJob] {
    stages
        .iter()
        .find(|stage| stage.id == stage_id)
        .map_or(&[], |stage| stage.codex_jobs.as_slice())
}

/// The job whose Codex thread `agent` reads, when the agent belongs to a native worker.
/// Joined on the thread alone, as the web dashboard does (`web/src/lib/codex-jobs.ts`), so an
/// agent not yet attributed a `unit_id` still shows its job's outcome in both views.
pub(super) fn job_of<'a>(agent: &AgentFlow, jobs: &'a [CodexJob]) -> Option<&'a CodexJob> {
    if agent.kind != AgentKind::Codex || agent.id.len() <= CODEX_ID_PREFIX.len() {
        return None;
    }
    jobs.iter().find(|job| {
        job.thread_id
            .as_deref()
            .is_some_and(|thread| format!("{CODEX_ID_PREFIX}{thread}").starts_with(&agent.id))
    })
}

/// The jobs no agent row of `stage` reads: a unit that failed before its thread started,
/// or whose rollout has not been found yet.
pub(super) fn unmatched<'a>(stage: &StageFlow, jobs: &'a [CodexJob]) -> Vec<&'a CodexJob> {
    jobs.iter()
        .filter(|job| {
            !stage
                .agents
                .iter()
                .any(|agent| job_of(agent, std::slice::from_ref(*job)).is_some())
        })
        .collect()
}

/// Whether `stage` has a job that is not finished and that no agent row reads: it keeps the
/// stage on the live pane. A finished one is listed under a shown stage but never pins it.
pub(super) fn has_open_job(stage: &StageFlow, jobs: &[CodexJob]) -> bool {
    unmatched(stage, jobs)
        .iter()
        .any(|job| job.phase != Phase::Terminal)
}

/// `23s`, `4m10s`, `1h02m`.
pub(super) fn duration_text(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m{:02}s", secs / 60, secs % 60),
        _ => format!("{}h{:02}m", secs / 3600, secs % 3600 / 60),
    }
}

/// A job's deadline: `25m` for whole minutes, else as [`duration_text`] (`1m30s`).
pub(super) fn deadline_text(secs: u64) -> String {
    let secs = i64::try_from(secs).unwrap_or(i64::MAX);
    if (60..3_600).contains(&secs) && secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        duration_text(secs)
    }
}

/// The outcome (`running` until the unit is terminal, `overdue` once it runs past its
/// deadline), how long the job ran (from its start to its end, or to `now` while it runs)
/// and its deadline: `running 4m28s / 25m`.
pub(super) fn state_text(job: &CodexJob, now: DateTime<Utc>) -> (String, Style) {
    let end = job.ended_at.unwrap_or(now);
    let secs = (end - job.started_at).num_seconds();
    let overdue = u64::try_from(secs).is_ok_and(|secs| secs > job.deadline_secs);
    let (word, color) = match job.outcome {
        // A terminal job never reads as running, even when its outcome went unrecorded.
        None if job.phase == Phase::Terminal => ("unknown", Color::Yellow),
        None if overdue => ("overdue", Color::Yellow),
        None => ("running", Color::Blue),
        Some(Outcome::Succeeded) => ("succeeded", Color::Green),
        Some(Outcome::Failed) => ("failed", Color::Red),
        Some(Outcome::Cancelled) => ("cancelled", Color::Yellow),
        Some(Outcome::TimedOut) => ("timed out", Color::Red),
        Some(Outcome::Unknown) => ("unknown", Color::Yellow),
    };
    (
        format!(
            "{word} {} / {}",
            duration_text(secs),
            deadline_text(job.deadline_secs)
        ),
        Style::default().fg(color),
    )
}

/// The model with its reasoning effort after it, e.g. `gpt-6.1-terra/xhigh`.
pub(super) fn model_text(model: Option<&str>, effort: Option<&str>) -> String {
    model_with_effort(model.unwrap_or("—"), effort)
}

#[cfg(test)]
#[path = "tokens_jobs_tests.rs"]
mod tests;
