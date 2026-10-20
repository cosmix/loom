//! What the operator is told to do for a stage that stopped and waits for
//! `loom stage retry`: a block, or failed acceptance.
//!
//! A retry before the stage's cause is dealt with only stops it again, so a
//! stage that has a cause the operator must act on first says so, and a
//! deliberate block offers the operator's other choices too.

use crate::commands::status::data::StageSummary;
use crate::context::untrusted::inline_safe;
use crate::models::failure::FailureType;
use crate::orchestrator::retry::should_auto_retry;

use super::attention_model::{failure_label, Alternative, Guidance};

/// How the primary command of a deliberate block is introduced.
const RETRY_WITH_ANSWER: &str =
    "you have done what the reason asks, retry with your answer (the next session receives it)";

/// What the operator edits in the `--context` of the retry that answers a block.
const ANSWER_PLACEHOLDER: &str = "<what you did and what it showed>";

/// A crash or timeout under the retry limit is requeued by the daemon once its
/// backoff elapses; any other failure waits for `loom stage retry`.
pub(super) fn blocked_guidance(stage: &StageSummary) -> Guidance {
    let max = retry_limit(stage);
    match (stage.failure_info.as_ref(), stage.close_reason.as_deref()) {
        (Some(failure), _) if should_auto_retry(&failure.failure_type, stage.retry_count, max) => {
            Guidance::automatic(format!(
                "auto-retry {} of {max} pending after a {}",
                stage.retry_count + 1,
                failure_label(&failure.failure_type)
            ))
        }
        // Every block writer clears `failure_info`, so a reason without one is a
        // deliberate block: the stage agent's, or the operator's own.
        (None, Some(reason)) => deliberate_block(stage, reason),
        (Some(failure), _) if needs_a_fix_first(&failure.failure_type) => {
            cause_first_guidance(stage)
        }
        _ => retry_guidance(stage),
    }
}

/// `loom stage retry` refuses a stage at its retry limit unless forced.
pub(super) fn retry_guidance(stage: &StageSummary) -> Guidance {
    let max = retry_limit(stage);
    if stage.retry_count < max {
        return Guidance::run(format!("loom stage retry {}", stage.id));
    }
    Guidance {
        note: Some(format!("retry limit reached ({}/{max})", stage.retry_count)),
        ..Guidance::run(format!("loom stage retry {} --force", stage.id))
    }
}

/// The stage's retry limit, with the default the daemon and `loom stage retry` apply.
fn retry_limit(stage: &StageSummary) -> u32 {
    stage.max_retries.unwrap_or(3)
}

/// A block whose reason the operator must act on. The retry that follows
/// carries the operator's answer in `--context`, the only way the next session
/// learns what was done: a plain retry starts it with no word of the reason.
///
/// The reason is a criterion that cannot pass where the stage runs more often
/// than not, so the operator may instead change or drop that criterion, or give
/// up on the stage. A rejection in human review is the operator's own verdict,
/// not a criterion, so it offers no amend.
fn deliberate_block(stage: &StageSummary, reason: &str) -> Guidance {
    let id = &stage.id;
    let rejected = rejected_in_review(stage, reason);
    let who = if rejected {
        "the operator (rejected in human review)"
    } else {
        "the stage agent"
    };
    let force = if stage.retry_count < retry_limit(stage) {
        ""
    } else {
        " --force"
    };
    let mut alternatives = Vec::new();
    if !rejected {
        alternatives.extend(criterion_alternatives(stage));
    }
    alternatives.push(Alternative::new(
        "you give up on the stage (its dependents stay blocked)",
        format!("loom stage skip {id}"),
    ));
    Guidance {
        command_when: Some(RETRY_WITH_ANSWER.to_owned()),
        alternatives,
        acceptance: if rejected {
            Vec::new()
        } else {
            stage.acceptance.clone()
        },
        note: Some(format!("blocked by {who}: {reason}")),
        ..Guidance::run(format!(
            "loom stage retry {id}{force} --context \"{ANSWER_PLACEHOLDER}\""
        ))
    }
}

/// Drop or replace an acceptance criterion; `loom stage amend` counts the
/// criteria from 0, and `loom status` and the stage modal list them with that
/// number. A criterion known to have failed gets both commands with its index
/// filled in; with none known the operator finds the index in the acceptance
/// list.
fn criterion_alternatives(stage: &StageSummary) -> Vec<Alternative> {
    let id = &stage.id;
    let failing = &stage.block_detail.failing_criteria;
    if failing.is_empty() {
        return vec![
            Alternative::new(
                "the reason is an acceptance criterion that cannot pass where the stage runs, \
                 drop it before retrying (find the criterion's index in the acceptance list)",
                format!("loom stage amend {id} --field acceptance --op delete --index <n>"),
            ),
            Alternative::new(
                "that criterion should run another way, replace it before retrying (same index)",
                format!(
                    "loom stage amend {id} --field acceptance --op replace --index <n> \
                     --value \"<new command>\""
                ),
            ),
        ];
    }
    failing
        .iter()
        .flat_map(|criterion| {
            let index = criterion.index;
            [
                Alternative::new(
                    format!(
                        "criterion {index} cannot pass where the stage runs, drop it before \
                         retrying"
                    ),
                    format!("loom stage amend {id} --field acceptance --op delete --index {index}"),
                ),
                Alternative::new(
                    format!("criterion {index} should run another way, replace it before retrying"),
                    format!(
                        "loom stage amend {id} --field acceptance --op replace --index {index} \
                         --value \"<new command>\""
                    ),
                ),
            ]
        })
        .collect()
}

/// Whether the operator's review rejection wrote this reason.
/// `loom stage human-review --reject` records its reason as both the review
/// reason and the close reason; `loom stage block` writes the close reason
/// alone. The review reason reaches the summary flattened to one line, so the
/// close reason is compared in the same form.
fn rejected_in_review(stage: &StageSummary, reason: &str) -> bool {
    stage.review_reason.as_deref() == Some(inline_safe(reason).as_str())
}

/// Failures whose cause lies in the setup around the agent, not in its work:
/// a retry before the setup is fixed fails the same way.
fn needs_a_fix_first(failure_type: &FailureType) -> bool {
    matches!(
        failure_type,
        FailureType::InfrastructureError
            | FailureType::SandboxSetupFailure
            | FailureType::StartupRefusal
    )
}

fn cause_first_guidance(stage: &StageSummary) -> Guidance {
    let cause = "retrying before the cause is fixed blocks the stage the same way; the cause \
                 is in the evidence (loom status --verbose)";
    let retry = retry_guidance(stage);
    let note = match retry.note.as_deref() {
        Some(limit) => format!("{limit}; {cause}"),
        None => cause.to_owned(),
    };
    Guidance {
        command_when: Some("you have fixed the cause named in the evidence".to_owned()),
        note: Some(note),
        ..retry
    }
}
