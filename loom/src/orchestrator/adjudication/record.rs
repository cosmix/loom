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
//! 3. the dispute's `request.md` must exist, so a verdict cannot invent the
//!    dispute it answers;
//! 4. `verdict.md` must not exist yet, so a recorded verdict cannot be
//!    overwritten with a different one before the daemon applies it.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::dispute::verdict_file;
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, update_stage};

use super::{attempt_count, persist_verdict, read_request, resolve_model, verdict};

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
    ensure_recordable(work_dir, stage_id, dispute_id)?;

    let raw = std::fs::read_to_string(verdict_path)
        .with_context(|| format!("Failed to read verdict file: {}", verdict_path.display()))?;
    record_validated(work_dir, stage_id, dispute_id, &raw, session_id)
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
    ensure_recordable(work_dir, stage_id, dispute_id)?;
    record_validated(work_dir, stage_id, dispute_id, raw, session_id)
}

/// Validate `raw` and either persist it or escalate the stage.
fn record_validated(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    raw: &str,
    session_id: Option<String>,
) -> Result<AdjudicateOutcome> {
    match verdict::parse_and_validate(raw) {
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
        // goes to a human instead and no verdict file is written.
        verdict::ValidationOutcome::Escalate { reason } => {
            update_stage(stage_id, work_dir, |s| {
                s.try_request_human_review(reason.clone()).ok();
                Ok(())
            })
            .context("Failed to escalate the stage after a degenerate verdict")?;
            Ok(AdjudicateOutcome::Escalated(reason))
        }
    }
}

/// Guards 2-4: the stage is under adjudication, the dispute exists, and no
/// verdict has been recorded for it yet.
fn ensure_recordable(work_dir: &Path, stage_id: &str, dispute_id: u32) -> Result<()> {
    let stage = load_stage(stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{stage_id}'"))?;
    if stage.status != StageStatus::NeedsAdjudication {
        bail!(
            "Stage '{stage_id}' is {}, not NeedsAdjudication, so no verdict can be recorded \
             against it.",
            stage.status
        );
    }
    read_request(work_dir, stage_id, dispute_id).with_context(|| {
        format!("No readable dispute {dispute_id} for stage '{stage_id}' to answer")
    })?;
    if verdict_file(&work_dir.join("disputes"), stage_id, dispute_id).exists() {
        bail!(
            "A verdict for stage '{stage_id}' dispute {dispute_id} has already been recorded; \
             it cannot be replaced."
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
