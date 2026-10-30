//! The issues a stage has moved past: every `failure_info`, `review_reason`
//! and merge conflict, archived into `Stage::issue_history` in the same write
//! that ends it, so the current fields describe only the current status and
//! nothing a stage ran into is lost.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{Stage, StageStatus};
use crate::models::failure::{FailureInfo, FailureType};

/// What a [`StageIssue`] was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IssueKind {
    /// A `failure_info`: a crash, a setup failure, a merge error, the cause of
    /// an escalation.
    Failure,
    /// A `review_reason`: why the stage waited for review, adjudication or
    /// input, or why its review was rejected.
    Review,
    /// The stage sat in `MergeConflict`.
    MergeConflict,
    /// One dispute's adjudication round. Never stored on the stage: the status
    /// collector reads it from the dispute's own records.
    Adjudication,
}

/// One ended issue. Every field is always serialized, so a reader of the
/// stage file or the status payload sees one shape for every kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageIssue {
    pub kind: IssueKind,
    /// The failure's type; `None` for every other kind.
    pub failure_type: Option<FailureType>,
    /// When the issue was recorded, where loom knows it: a failure's
    /// `detected_at`, when the stage entered `MergeConflict`, a dispute's
    /// filing time.
    pub detected_at: Option<DateTime<Utc>>,
    /// The failure's evidence lines, or the reason's lines, in full.
    pub evidence: Vec<String>,
    /// When the issue stopped describing the stage.
    pub ended_at: DateTime<Utc>,
    /// What ended it: the transition (`NeedsHumanReview -> Queued`), the
    /// command, or the verdict.
    pub ended_by: String,
    /// The dispute an adjudication round belongs to.
    pub dispute_id: Option<u32>,
}

impl StageIssue {
    fn ended_now(kind: IssueKind, evidence: Vec<String>, ended_by: &str) -> Self {
        Self {
            kind,
            failure_type: None,
            detected_at: None,
            evidence,
            ended_at: Utc::now(),
            ended_by: ended_by.to_owned(),
            dispute_id: None,
        }
    }

    fn failure(failure: FailureInfo, ended_by: &str) -> Self {
        Self {
            failure_type: Some(failure.failure_type),
            detected_at: Some(failure.detected_at),
            ..Self::ended_now(IssueKind::Failure, failure.evidence, ended_by)
        }
    }
}

/// Entering one of these starts the stage over or ends it, so whatever the
/// status it left recorded no longer describes it. `WaitingForDeps` is
/// entered only by `loom stage reset`.
fn supersedes_reasons(status: &StageStatus) -> bool {
    matches!(
        status,
        StageStatus::WaitingForDeps
            | StageStatus::Queued
            | StageStatus::Executing
            | StageStatus::Completed
            | StageStatus::Skipped
    )
}

impl Stage {
    /// Archive `failure_info` into `issue_history`, ended now by `ended_by`.
    pub fn retire_failure(&mut self, ended_by: &str) {
        if let Some(failure) = self.failure_info.take() {
            self.issue_history
                .push(StageIssue::failure(failure, ended_by));
        }
    }

    /// Record `failure` as the current failure, archiving a different one it
    /// replaces. A re-record of the same failure (same type and evidence, as
    /// the daemon's merge retry writes every tick) keeps the existing record
    /// and its `detected_at`, so repeating a failure never grows the history.
    pub fn replace_failure(&mut self, failure: FailureInfo) {
        let same = self.failure_info.as_ref().is_some_and(|current| {
            current.failure_type == failure.failure_type && current.evidence == failure.evidence
        });
        if !same {
            self.retire_failure("replaced by a newer failure");
            self.failure_info = Some(failure);
        }
    }

    /// Archive `review_reason` into `issue_history`, ended now by `ended_by`.
    pub fn retire_review_reason(&mut self, ended_by: &str) {
        if let Some(reason) = self.review_reason.take() {
            let lines = reason.lines().map(str::to_owned).collect();
            self.issue_history
                .push(StageIssue::ended_now(IssueKind::Review, lines, ended_by));
        }
    }

    /// Record `reason` as the review reason, archiving a different one it
    /// replaces.
    pub fn replace_review_reason(&mut self, reason: String) {
        if self.review_reason.as_deref() != Some(reason.as_str()) {
            self.retire_review_reason("replaced by a newer review reason");
            self.review_reason = Some(reason);
        }
    }

    /// Put the stage back in `WaitingForDeps` for `loom stage reset`, the one
    /// sanctioned way into that status: it has no incoming edge in the
    /// transition table, so this is not a forced bypass and does not log as
    /// one. Archives what the left status recorded.
    pub fn reset_to_waiting_for_deps(&mut self) {
        let from = std::mem::replace(&mut self.status, StageStatus::WaitingForDeps);
        tracing::debug!(stage_id = %self.id, from = %from, "Stage reset to WaitingForDeps");
        self.retire_issues_of(&from);
        self.updated_at = Utc::now();
    }

    /// Archive what the status the stage just left recorded about itself.
    /// Both status writers call this after assigning; a writer that records a
    /// reason for the new status sets it after its transition.
    ///
    /// A typed merge block outside `MergeBlocked` is dropped, its failure
    /// archived as ended by the transition. A failure ends when the stage is
    /// reset, requeued, runs again, finishes or conflicts on merge (the
    /// conflict is then the whole story; no conflict path records a
    /// `failure_info`). A review reason ends with each of those but the
    /// conflict. Leaving `MergeConflict` archives the conflict itself,
    /// detected when it was entered.
    pub(super) fn retire_issues_of(&mut self, from: &StageStatus) {
        let ended_by = format!("{from} -> {}", self.status);
        if self.status != StageStatus::MergeBlocked && self.merge.block.take().is_some() {
            self.retire_failure(&ended_by);
        }
        if *from == self.status {
            return;
        }
        // A refusal describes one failure: once the stage completes or merges
        // again it must not brief a later session. A hold goes to
        // `NeedsHumanReview`, which keeps it for the approved session.
        if matches!(
            self.status,
            StageStatus::Completed | StageStatus::MergeConflict | StageStatus::MergeBlocked
        ) {
            self.merge.handback_refusal = None;
        }
        if *from == StageStatus::MergeConflict {
            self.issue_history.push(StageIssue {
                detected_at: self.merge.conflict_since.take(),
                ..StageIssue::ended_now(IssueKind::MergeConflict, Vec::new(), &ended_by)
            });
        }
        if self.status == StageStatus::MergeConflict {
            self.merge.conflict_since = Some(Utc::now());
        }
        let superseded = supersedes_reasons(&self.status);
        if superseded || self.status == StageStatus::MergeConflict {
            self.retire_failure(&ended_by);
        }
        if superseded {
            self.retire_review_reason(&ended_by);
        }
    }
}

#[cfg(test)]
#[path = "issue_history_tests.rs"]
mod tests;
