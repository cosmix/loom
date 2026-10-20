//! Pure attention-state data shared by the text and interactive renderers.
//!
//! This module decides which stages need a human and the recovery information
//! to show, leaving each renderer responsible only for presentation.

use serde::{Deserialize, Serialize};

use crate::commands::status::data::{
    CompletionBlockerState, CompletionBlockerSummary, DisputeLine, FailingCriterion, StageSummary,
};
use crate::models::failure::FailureType;
use crate::models::session::{SessionExitReason, SessionType};
use crate::models::stage::{StageStatus, StageType};
use crate::orchestrator::terminal::tmux::history_relpath;

mod merge;

use super::blocked_guidance::{blocked_guidance, retry_guidance};
use merge::{merge_block_guidance, merge_guidance};

/// Display-ready information for one stage that needs human attention.
#[derive(Debug, Clone)]
pub struct AttentionEntry {
    pub id: String,
    pub name: String,
    pub label: &'static str,
    /// A shell command the operator should run. `None` when no single command
    /// applies or loom is handling the state.
    pub command: Option<String>,
    /// When to run `command`, phrased to follow "Once ": set when the command
    /// is the step after something else the operator must do first.
    pub command_when: Option<String>,
    /// Other commands for other situations, in the order to offer them.
    pub alternatives: Vec<Alternative>,
    /// The acceptance criteria the entry offers to drop or replace, in the order
    /// `loom stage amend --index` counts them from 0. Empty when it offers none.
    pub acceptance: Vec<String>,
    /// What loom is doing, or what the operator should do when no single
    /// command fits. Prose, never a command meant for copying.
    pub note: Option<String>,
    /// Loom is handling this state itself; the operator need not act.
    pub automatic: bool,
    pub failure_type: Option<FailureType>,
    pub evidence: Vec<String>,
    /// Why a stage awaiting human review was flagged; `None` for every other state.
    pub review_reason: Option<String>,
    /// Why the stage needs the operator now: a block's close reason, or a
    /// review's reason. Never an earlier state's leftover review reason.
    pub reason: Option<String>,
    /// Acceptance criteria that failed in the stage's last completion attempt.
    pub failing_criteria: Vec<FailingCriterion>,
    /// Disputes already ruled on for a blocked stage, oldest first.
    pub disputes: Vec<DisputeLine>,
    /// The session whose terminal history the stage left, when it left one.
    pub terminal_history: Option<String>,
    pub cleanup_warning: Option<String>,
    pub has_human_review_choices: bool,
    pub dispute_count: Option<u32>,
    pub judge_heartbeat_secs: Option<u64>,
    pub completion_blocker: Option<CompletionBlockerSummary>,
    pub outgoing_session_exit_reason: Option<SessionExitReason>,
}

/// A command for a situation other than the one the entry's primary
/// `command` is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alternative {
    /// The situation, phrased to follow "If ".
    pub when: String,
    /// The command to run in that situation.
    pub command: String,
}

impl Alternative {
    pub(super) fn new(when: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            when: when.into(),
            command: command.into(),
        }
    }
}

/// The guidance fields of an [`AttentionEntry`].
#[derive(Default)]
pub(super) struct Guidance {
    pub(super) command: Option<String>,
    pub(super) command_when: Option<String>,
    pub(super) alternatives: Vec<Alternative>,
    pub(super) acceptance: Vec<String>,
    pub(super) note: Option<String>,
    pub(super) automatic: bool,
}

impl Guidance {
    /// The operator should run `command`.
    pub(super) fn run(command: impl Into<String>) -> Self {
        Self {
            command: Some(command.into()),
            ..Self::default()
        }
    }

    /// The operator should act as `note` says; no single command fits.
    pub(super) fn manual(note: impl Into<String>) -> Self {
        Self {
            note: Some(note.into()),
            ..Self::default()
        }
    }

    /// Loom is handling the state as `note` says.
    pub(super) fn automatic(note: impl Into<String>) -> Self {
        Self {
            automatic: true,
            ..Self::manual(note)
        }
    }
}

const INPUT_NOTE: &str = "the agent is waiting on a question: answer it in the stage's terminal";
const ADJUDICATION_NOTE: &str = "a judge session is ruling on the open disputes";

impl AttentionEntry {
    /// An entry for `stage` carrying only its identity, label and guidance.
    fn new(stage: &StageSummary, label: &'static str, guidance: Guidance) -> Self {
        Self {
            id: stage.id.clone(),
            name: stage.name.clone(),
            label,
            command: guidance.command,
            command_when: guidance.command_when,
            alternatives: guidance.alternatives,
            acceptance: guidance.acceptance,
            note: guidance.note,
            automatic: guidance.automatic,
            failure_type: None,
            evidence: Vec::new(),
            review_reason: None,
            reason: None,
            failing_criteria: Vec::new(),
            disputes: Vec::new(),
            terminal_history: None,
            cleanup_warning: None,
            has_human_review_choices: false,
            dispute_count: None,
            judge_heartbeat_secs: None,
            completion_blocker: None,
            outgoing_session_exit_reason: stage.outgoing_session_exit_reason,
        }
    }
}

impl AttentionEntry {
    /// The note as a terminal shows it: the note, then where the stage's last
    /// terminal history can be read with `less`. A browser links the history
    /// instead, so the web wire carries `note` alone.
    pub fn text_note(&self) -> Option<String> {
        let history = self.terminal_history.as_deref().map(|session| {
            format!(
                "terminal history of its last session: less -R .loom/work/{}",
                history_relpath(session).display()
            )
        });
        match (self.note.clone(), history) {
            (Some(note), Some(history)) => Some(format!("{note}\n{history}")),
            (note, history) => note.or(history),
        }
    }
}

/// The three decisions a stage awaiting human review accepts, as full
/// commands for stage `id`, each with what it does.
pub fn human_review_choices(id: &str) -> [(String, &'static str); 3] {
    [
        (
            format!("loom stage human-review {id} --approve"),
            "queue a fresh session with fresh fix attempts",
        ),
        (
            format!("loom stage human-review {id} --force-complete"),
            "skip acceptance and mark completed",
        ),
        (
            format!("loom stage human-review {id} --reject \"<reason>\""),
            "block the stage",
        ),
    ]
}

/// Return the attention entries in the same order as their input stages.
pub fn attention_entries(stages: &[StageSummary]) -> Vec<AttentionEntry> {
    stages.iter().filter_map(attention_entry).collect()
}

fn attention_entry(stage: &StageSummary) -> Option<AttentionEntry> {
    // Unrestored changes are data at risk, so they outrank every other state.
    if let Some(backup_refs) = stage.stash_warning.as_deref() {
        return Some(stash_entry(stage, backup_refs));
    }
    if stage.cleanup_warning.is_some() {
        return Some(cleanup_entry(stage));
    }
    if let Some(blocker) = stage.completion_blocker.clone() {
        return Some(completion_blocker_entry(stage, blocker));
    }

    status_entry(stage)
}

/// `backup_refs` is every ref a merge kept stashed changes in, joined by ", ".
fn stash_entry(stage: &StageSummary, backup_refs: &str) -> AttentionEntry {
    let guidance = Guidance {
        note: Some(format!(
            "the merge of {} stashed your uncommitted changes in the main checkout and could \
             not put them back: restore them with `git stash list` / `git stash pop` (backups: \
             {backup_refs}), then delete each backup with `git update-ref -d <ref>`",
            stage.id
        )),
        ..Guidance::run("git stash list")
    };
    AttentionEntry::new(stage, "STASH NOT RESTORED", guidance)
}

fn cleanup_entry(stage: &StageSummary) -> AttentionEntry {
    let guidance = Guidance::run(format!("loom worktree remove {}", stage.id));
    AttentionEntry {
        cleanup_warning: stage.cleanup_warning.clone(),
        ..AttentionEntry::new(stage, "CLEANUP FAILED", guidance)
    }
}

/// A completion blocker's `next_action` is prose. The daemon still owns an
/// `Executing` stage; a parked stage takes the review decisions once the
/// blocker is confirmed.
fn completion_blocker_entry(
    stage: &StageSummary,
    blocker: CompletionBlockerSummary,
) -> AttentionEntry {
    let label = completion_blocker_label(blocker.state);
    let guidance = Guidance {
        automatic: stage.status == StageStatus::Executing,
        ..Guidance::manual(blocker.next_action.clone())
    };
    AttentionEntry {
        has_human_review_choices: stage.status == StageStatus::NeedsHumanReview
            && blocker.state == CompletionBlockerState::Blocked,
        completion_blocker: Some(blocker),
        ..AttentionEntry::new(stage, label, guidance)
    }
}

fn completion_blocker_label(state: CompletionBlockerState) -> &'static str {
    match state {
        CompletionBlockerState::Pending => "COMPLETION PENDING",
        CompletionBlockerState::Blocked => "COMPLETION BLOCKED",
        CompletionBlockerState::OwnershipUnknown => "WRITER UNCONFIRMED",
    }
}

fn status_entry(stage: &StageSummary) -> Option<AttentionEntry> {
    let (label, guidance) = status_guidance(stage)?;
    let guidance_automatic = guidance.automatic;
    let is_adjudicating = stage.status == StageStatus::NeedsAdjudication;
    let (dispute_count, judge_heartbeat_secs) = if is_adjudicating {
        (Some(stage.dispute_count), stage.judge_heartbeat_secs)
    } else {
        (None, None)
    };
    let (failure_type, evidence) = stage
        .failure_info
        .as_ref()
        .map_or((None, Vec::new()), |failure| {
            (Some(failure.failure_type.clone()), failure.evidence.clone())
        });

    let in_review = stage.status == StageStatus::NeedsHumanReview;
    let terminal_history = stage
        .terminal_history_session
        .clone()
        .filter(|_| stage.status == StageStatus::Blocked && !guidance_automatic);
    let review_reason = stage.review_reason.clone().filter(|_| in_review);
    let reason = match stage.status {
        StageStatus::Blocked => stage.close_reason.clone(),
        _ => review_reason.clone(),
    };

    Some(AttentionEntry {
        failure_type,
        evidence,
        review_reason,
        reason,
        failing_criteria: stage.block_detail.failing_criteria.clone(),
        disputes: stage.block_detail.disputes.clone(),
        terminal_history,
        has_human_review_choices: in_review,
        dispute_count,
        judge_heartbeat_secs,
        ..AttentionEntry::new(stage, label, guidance)
    })
}

fn status_guidance(stage: &StageSummary) -> Option<(&'static str, Guidance)> {
    Some(match stage.status {
        StageStatus::Blocked => ("BLOCKED", blocked_guidance(stage)),
        StageStatus::MergeConflict => ("MERGE CONFLICT", merge_guidance(stage)),
        StageStatus::CompletedWithFailures => ("ACCEPTANCE FAILED", retry_guidance(stage)),
        StageStatus::MergeBlocked => match stage.merge_block.as_deref() {
            Some(sentence) => ("MERGE BLOCKED", merge_block_guidance(sentence)),
            None => ("MERGE ERROR", merge_guidance(stage)),
        },
        StageStatus::NeedsHumanReview => ("NEEDS REVIEW", Guidance::default()),
        StageStatus::WaitingForInput => ("NEEDS INPUT", Guidance::manual(INPUT_NOTE)),
        StageStatus::NeedsAdjudication => ("ADJUDICATING", Guidance::automatic(ADJUDICATION_NOTE)),
        _ => return None,
    })
}

/// A `Standard` stage in the contract-writer phase: an `Executing` stage
/// whose live session is the contract-test writer that runs ahead of the
/// stage's own `Stage` session on a v2 stage with `contracts`. Gated on
/// `StageType::Standard` because that phase only exists there; a `Contract`
/// session found on any other stage type is a coherence anomaly, not a
/// phase, so it keeps the ordinary "{session_type} session" warning tag.
pub fn is_contract_phase(stage: &StageSummary) -> bool {
    stage.status == StageStatus::Executing
        && stage.session_type == Some(SessionType::Contract)
        && stage.stage_type == StageType::Standard
}

/// Short status-line label for a blocked stage's failure type.
pub fn failure_label(failure_type: &FailureType) -> &'static str {
    match failure_type {
        FailureType::SessionCrash => "crash",
        FailureType::TestFailure => "test",
        FailureType::BuildFailure => "build",
        FailureType::CodeError => "code",
        FailureType::Timeout => "timeout",
        FailureType::ContextExhausted => "context",
        FailureType::UserBlocked => "user",
        FailureType::MergeConflict => "merge",
        FailureType::InfrastructureError => "infra",
        FailureType::SandboxSetupFailure => "sandbox",
        FailureType::StartupRefusal => "startup",
        FailureType::Unknown => "error",
    }
}

#[cfg(test)]
#[path = "attention_model_tests.rs"]
mod tests;
