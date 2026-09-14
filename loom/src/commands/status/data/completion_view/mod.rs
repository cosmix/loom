use serde::{Deserialize, Serialize};

use crate::fs::work_dir::WorkDir;
use crate::handoff::{current_blocker, CompletionBlocker, CompletionCheckpoint};
use crate::models::session::{Session, SessionExitReason};
use crate::models::stage::{Stage, StageStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionBlockerState {
    Pending,
    Blocked,
    OwnershipUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionBlockerSummary {
    pub state: CompletionBlockerState,
    pub fingerprint: String,
    pub failure_code: String,
    pub summary: Option<String>,
    pub commit: String,
    pub repeat_count: u32,
    pub first_observed_at: Option<String>,
    pub last_observed_at: Option<String>,
    pub next_action: String,
}

pub fn exit_reason_label(reason: SessionExitReason) -> &'static str {
    match reason {
        SessionExitReason::Completed => "completed",
        SessionExitReason::Crashed => "crashed",
        SessionExitReason::ContextCeiling => "context ceiling",
        SessionExitReason::Stalled => "stalled",
        SessionExitReason::OperatorStop => "operator stop",
        SessionExitReason::CriteriaBlocked => "criteria blocked",
        SessionExitReason::Replaced => "replaced",
    }
}

pub fn blocker_activity_text(blocker: &CompletionBlockerSummary) -> String {
    let detail = blocker.summary.as_deref().unwrap_or(&blocker.failure_code);
    let prefix = match blocker.state {
        CompletionBlockerState::Pending => "completion pending",
        CompletionBlockerState::Blocked => "completion blocked",
        CompletionBlockerState::OwnershipUnknown => "completion blocked, writer unconfirmed",
    };
    format!("{prefix}: {detail}")
}

impl CompletionBlockerSummary {
    pub fn activity_text(&self) -> String {
        blocker_activity_text(self)
    }
}

impl SessionExitReason {
    pub fn status_label(self) -> &'static str {
        exit_reason_label(self)
    }
}

pub fn completion_blocker_summary(
    stage: &Stage,
    outgoing: Option<&Session>,
    checkpoint: &CompletionCheckpoint,
    current_commit: Option<&str>,
) -> Option<CompletionBlockerSummary> {
    let blocker = current_blocker(checkpoint, stage, current_commit)
        .or_else(|| capacity_blocker(checkpoint, stage, current_commit))?;
    let repeat_count = checkpoint.repeat_count();
    let (state, next_action) = blocker_state(stage, outgoing, checkpoint, repeat_count)?;

    Some(CompletionBlockerSummary {
        state,
        fingerprint: blocker.short_fingerprint().to_owned(),
        failure_code: blocker.external_failure_code.clone(),
        summary: blocker.summary.clone(),
        commit: short(&blocker.commit),
        repeat_count,
        first_observed_at: checkpoint.first_observed_at.clone(),
        last_observed_at: checkpoint.last_observed_at.clone(),
        next_action,
    })
}

pub fn outgoing_exit_reason(outgoing: Option<&Session>) -> Option<SessionExitReason> {
    outgoing
        .filter(|session| session.status.is_terminal())
        .and_then(|session| session.exit_reason)
}

pub(super) fn collect_completion_view(
    stage: &Stage,
    outgoing: Option<&Session>,
    work_dir: &WorkDir,
) -> (Option<SessionExitReason>, Option<CompletionBlockerSummary>) {
    let exit_reason = outgoing_exit_reason(outgoing);
    let Some(session_id) = stage.session.as_deref() else {
        return (exit_reason, None);
    };
    if !matches!(
        stage.status,
        StageStatus::Executing | StageStatus::NeedsHumanReview
    ) {
        return (exit_reason, None);
    }
    let checkpoint =
        crate::handoff::load_trusted_session_checkpoint(&stage.id, session_id, work_dir.root())
            .ok()
            .flatten();
    let summary = checkpoint.as_ref().and_then(|checkpoint| {
        let current_commit = checkpoint.blocker.as_ref().and_then(|_| {
            work_dir
                .main_project_root()
                .and_then(|repo_root| crate::handoff::expected_stage_commit(stage, &repo_root).ok())
        });
        completion_blocker_summary(stage, outgoing, checkpoint, current_commit.as_deref())
    });
    (exit_reason, summary)
}

fn blocker_state(
    stage: &Stage,
    outgoing: Option<&Session>,
    checkpoint: &CompletionCheckpoint,
    repeat_count: u32,
) -> Option<(CompletionBlockerState, String)> {
    let code = &checkpoint.blocker.as_ref()?.external_failure_code;
    match stage.status {
        StageStatus::Executing if repeat_count == 1 && !checkpoint.capacity_exhausted => Some((
            CompletionBlockerState::Pending,
            format!("watching: a repeat of {code} parks this stage"),
        )),
        StageStatus::Executing => Some((
            CompletionBlockerState::Blocked,
            "stopping the writer before parking; do not reassign".to_string(),
        )),
        StageStatus::NeedsHumanReview => review_state(stage, outgoing, code),
        _ => None,
    }
}

fn capacity_blocker<'a>(
    checkpoint: &'a CompletionCheckpoint,
    stage: &Stage,
    current_commit: Option<&str>,
) -> Option<&'a CompletionBlocker> {
    let blocker = checkpoint.blocker.as_ref()?;
    let latest = checkpoint.latest.as_ref()?;
    let latest_blocker = CompletionBlocker::from_attempt(latest).ok()?;
    (checkpoint.capacity_exhausted
        && !checkpoint.conflict
        && checkpoint.accepted.is_none()
        && checkpoint.stage_id == stage.id
        && stage.session.as_deref() == Some(checkpoint.session_id.as_str())
        && latest_blocker.fingerprint == blocker.fingerprint
        && current_commit == Some(blocker.commit.as_str()))
    .then_some(blocker)
}

fn review_state(
    stage: &Stage,
    outgoing: Option<&Session>,
    code: &str,
) -> Option<(CompletionBlockerState, String)> {
    if outgoing.is_some_and(|session| session.status.is_terminal()) {
        return Some((
            CompletionBlockerState::Blocked,
            format!("fix {code}, then retry or reset the stage"),
        ));
    }
    let session_id = stage.session.as_deref()?;
    Some((
        CompletionBlockerState::OwnershipUnknown,
        format!("confirm session {session_id} has exited before reassigning"),
    ))
}

fn short(value: &str) -> String {
    value.chars().take(12).collect()
}

#[cfg(test)]
mod tests;
