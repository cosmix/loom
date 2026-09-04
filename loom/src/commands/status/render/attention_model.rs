//! Pure attention-state data shared by the text and interactive renderers.
//!
//! This module decides which stages need a human and the recovery information
//! to show, leaving each renderer responsible only for presentation.

use crate::commands::status::data::StageSummary;
use crate::models::failure::FailureType;
use crate::models::stage::StageStatus;

/// Display-ready information for one stage that needs human attention.
#[derive(Debug, Clone)]
pub struct AttentionEntry {
    pub id: String,
    pub name: String,
    pub label: &'static str,
    pub hint: String,
    pub failure_type: Option<FailureType>,
    pub evidence: Vec<String>,
    pub review_reason: Option<String>,
    pub cleanup_warning: Option<String>,
    pub has_human_review_choices: bool,
    pub dispute_count: Option<u32>,
    pub judge_heartbeat_secs: Option<u64>,
}

/// Return the attention entries in the same order as their input stages.
pub fn attention_entries(stages: &[StageSummary]) -> Vec<AttentionEntry> {
    stages.iter().filter_map(attention_entry).collect()
}

fn attention_entry(stage: &StageSummary) -> Option<AttentionEntry> {
    if let Some(cleanup_warning) = stage.cleanup_warning.clone() {
        return Some(AttentionEntry {
            id: stage.id.clone(),
            name: stage.name.clone(),
            label: "CLEANUP FAILED",
            hint: format!("loom worktree remove {}", stage.id),
            failure_type: None,
            evidence: Vec::new(),
            review_reason: None,
            cleanup_warning: Some(cleanup_warning),
            has_human_review_choices: false,
            dispute_count: None,
            judge_heartbeat_secs: None,
        });
    }

    let (label, hint, has_human_review_choices, is_adjudicating) =
        attention_status(&stage.status, &stage.id)?;
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

    Some(AttentionEntry {
        id: stage.id.clone(),
        name: stage.name.clone(),
        label,
        hint,
        failure_type,
        evidence,
        review_reason: stage.review_reason.clone(),
        cleanup_warning: None,
        has_human_review_choices,
        dispute_count,
        judge_heartbeat_secs,
    })
}

fn attention_status(status: &StageStatus, id: &str) -> Option<(&'static str, String, bool, bool)> {
    let (label, command, has_human_review_choices, is_adjudicating) = match status {
        StageStatus::Blocked => ("BLOCKED", "retry", false, false),
        StageStatus::MergeConflict => ("MERGE CONFLICT", "merge", false, false),
        StageStatus::CompletedWithFailures => ("ACCEPTANCE FAILED", "retry", false, false),
        StageStatus::MergeBlocked => ("MERGE ERROR", "merge", false, false),
        StageStatus::NeedsHumanReview => ("NEEDS REVIEW", "human-review", true, false),
        StageStatus::WaitingForInput => ("NEEDS INPUT", "resume", false, false),
        StageStatus::NeedsAdjudication => ("ADJUDICATING", "status --verbose", false, true),
        _ => return None,
    };
    let hint = if is_adjudicating {
        format!("loom {command}")
    } else {
        format!("loom stage {command} {id}")
    };
    Some((label, hint, has_human_review_choices, is_adjudicating))
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
