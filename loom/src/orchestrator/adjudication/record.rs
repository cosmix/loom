//! Recording an adjudication verdict: the guarded write behind `verdict.md`.
//!
//! Two callers share it. `loom stage adjudicate` calls it directly in legacy
//! and operator mode; the daemon's inbox drain calls it for a verdict relayed
//! from a judge's sandbox, after confirming the relay came from the live
//! adjudication session for the stage.
//!
//! # Trust boundary
//!
//! `verdict.md` exists apart from `request.md` so that the agent whose stage
//! is under dispute cannot approve its own criterion (see
//! `doc/loom/knowledge/conventions.md` § "Dispute File Ownership Convention").
//! An adjudication session is a different session, spawned by the daemon, so
//! it is not the party that rule excludes — but this write must not become a
//! general-purpose "write any verdict" tool either. Four guards keep it
//! narrow:
//!
//! 1. it refuses a stage worktree session ([`refuse_worktree_session`]):
//!    `LOOM_WORKTREE_PATH` is exported for stage sessions and for no other
//!    kind, and the daemon reads the same fact from the session record's
//!    `worktree_path`. That is exactly the party the split excludes — and the
//!    one that can be live in `NeedsAdjudication`, since filing a dispute does
//!    not end its session;
//! 2. the stage must be in `NeedsAdjudication`, so a verdict cannot be
//!    injected against a stage that is executing, completed, or merged;
//! 3. the dispute's `request.md` must exist and the dispute must not be
//!    closed, so a verdict cannot invent the dispute it answers or revive one
//!    an escalation abandoned (`closed_disputes.rs`);
//! 4. `verdict.md` must not exist yet, so a recorded verdict cannot be
//!    overwritten with a different one before the daemon applies it.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::dispute::{dispute_dir, verdict_file, DisputeKind};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::{load_stage, update_stage};

use super::{
    attempt_count, close_open_disputes, is_closed, persist_verdict, read_request, resolve_model,
    verdict,
};

/// What recording the verdict did.
#[derive(Debug, PartialEq, Eq)]
pub enum AdjudicateOutcome {
    /// `verdict.md` was written and awaits the daemon's next tick.
    Recorded,
    /// The verdict was too degenerate to act on; the stage was escalated to
    /// `NeedsHumanReview` with this reason instead.
    Escalated(String),
}

/// Guard 1: refuse a verdict from a stage worktree session.
///
/// A pure function of the worktree path the caller observed, so the rule is
/// testable without mutating the process environment: an absent (or blank)
/// path is the only acceptable state.
pub fn refuse_worktree_session(worktree_path: Option<&str>) -> Result<()> {
    let Some(path) = worktree_path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(());
    };
    bail!(
        "This session runs inside the stage worktree at {path}, so it cannot record an \
         adjudication verdict: a stage may not judge its own disputed criterion. Verdicts \
         come from the adjudication session the orchestrator spawns for the dispute."
    )
}

/// The guarded write for a verdict held in a file, against an explicit
/// state-directory root so it is testable.
pub fn record_verdict(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    verdict_path: &Path,
    session_id: Option<String>,
) -> Result<AdjudicateOutcome> {
    let (stage, kind) = ensure_recordable(work_dir, stage_id, dispute_id)?;

    let raw = std::fs::read_to_string(verdict_path)
        .with_context(|| format!("Failed to read verdict file: {}", verdict_path.display()))?;
    let outcome = validated(work_dir, &stage, &kind, &raw)?;
    record_outcome(work_dir, stage_id, dispute_id, outcome, session_id)
}

/// The guarded write for a verdict already read — the raw JSON a judge
/// relayed. Guards 2-4 run here; guard 1 is the caller's, since only the
/// caller knows where the verdict came from.
pub fn record_verdict_text(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    raw: &str,
    session_id: Option<String>,
) -> Result<AdjudicateOutcome> {
    let (stage, kind) = ensure_recordable(work_dir, stage_id, dispute_id)?;
    let outcome = validated(work_dir, &stage, &kind, raw)?;
    record_outcome(work_dir, stage_id, dispute_id, outcome, session_id)
}

/// `raw` validated as the verdict on a dispute of `kind` against `stage`. A
/// verdict refused outright (a freeze refusal's, see
/// `verdict::parse_and_validate_in`) is an `Err` the judge reads, and nothing
/// is written.
fn validated(
    work_dir: &Path,
    stage: &Stage,
    kind: &DisputeKind,
    raw: &str,
) -> Result<verdict::ValidationOutcome> {
    let context = verdict::RecordContext { work_dir, stage };
    verdict::parse_and_validate_in(raw, kind, &context).map_err(|problem| {
        anyhow::anyhow!(
            "The verdict on stage '{}' was not recorded: {problem}. Correct the JSON and run \
             `loom stage adjudicate` again.",
            stage.id
        )
    })
}

/// Persist a validated verdict, or escalate the stage when it is degenerate.
fn record_outcome(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    outcome: verdict::ValidationOutcome,
    session_id: Option<String>,
) -> Result<AdjudicateOutcome> {
    match outcome {
        verdict::ValidationOutcome::Verdict(v) => {
            let attempt = attempt_count(work_dir, stage_id, dispute_id).max(1);
            persist_verdict(
                work_dir,
                stage_id,
                dispute_id,
                &v,
                &resolve_model(work_dir),
                attempt,
                session_id,
            )
            .context("Failed to write the verdict record")?;
            Ok(AdjudicateOutcome::Recorded)
        }
        // Degenerate output (e.g. needs-more-evidence with no questions) would
        // loop the evidence round forever if it were recorded, so the stage
        // goes to a human instead and no verdict file is written. The disputes
        // it leaves open are closed, as for any escalation.
        verdict::ValidationOutcome::Escalate { reason } => {
            let stage = update_stage(stage_id, work_dir, |s| {
                s.try_request_human_review(reason.clone()).ok();
                Ok(())
            })
            .context("Failed to escalate the stage after a degenerate verdict")?;
            if stage.status == StageStatus::NeedsHumanReview {
                close_open_disputes(work_dir, stage_id);
            }
            Ok(AdjudicateOutcome::Escalated(reason))
        }
    }
}

/// Guards 2-4: the stage is under adjudication, the dispute exists and is
/// open, and no verdict has been recorded for it yet. Returns the stage and
/// the dispute's kind, which decide the verdicts it takes.
fn ensure_recordable(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
) -> Result<(Stage, DisputeKind)> {
    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;
    if stage.status != StageStatus::NeedsAdjudication {
        bail!(
            "Stage '{stage_id}' is {}, not NeedsAdjudication, so no verdict can be recorded \
             against it.",
            stage.status
        );
    }
    let request = read_request(work_dir, stage_id, dispute_id).with_context(|| {
        format!("No readable dispute {dispute_id} for stage '{stage_id}' to answer")
    })?;
    let disputes_root = work_dir.join("disputes");
    if is_closed(&dispute_dir(&disputes_root, stage_id, dispute_id)) {
        bail!(
            "Stage '{stage_id}' dispute {dispute_id} was closed when the stage was escalated \
             to human review; it takes no verdict."
        );
    }
    if verdict_file(&disputes_root, stage_id, dispute_id).exists() {
        bail!(
            "A verdict for stage '{stage_id}' dispute {dispute_id} has already been recorded; \
             it cannot be replaced."
        );
    }
    Ok((stage, request.kind))
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
