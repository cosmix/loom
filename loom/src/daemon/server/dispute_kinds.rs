//! Server-side handler for `Request::FileDispute` (DESIGN D15): a plan v2
//! stage disputes review findings, a frozen contract or test-integrity events.
//!
//! It files under the same per-stage lock, id allocation and create-new
//! `request.md` as `handle_dispute_criteria` (`dispute_store.rs`), after checks
//! of its own:
//!
//! 1. the stage is a plan v2 stage: a v1 stage disputes only its criteria;
//! 2. every id names something that exists now, as the daemon derives it
//!    itself: an open finding (own or carried), a frozen contract, or a current
//!    test-integrity event. The evidence `request.md` records is the daemon's
//!    own snapshot of those ids, never the client's copy;
//! 3. the kind's budget (`models::stage::dispute_budgets`); an exhausted one
//!    escalates the stage to NeedsHumanReview and files nothing.
//!
//! A refusal is a `Response::Error`, never an `Err`: the spool drain retries
//! an `Err` on every tick, and a dispute refused once is refused forever.

use anyhow::{bail, Context, Result};
use chrono::Utc;
use std::path::Path;

use super::dispute_store::{escalate_to_human_review, lock_stage_disputes, write_request};
use crate::daemon::protocol::Response;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{select_events, select_findings, DisputeKind, DisputeRequest};
use crate::models::stage::dispute_budgets::{self, MAX_DISPUTES_PER_KIND};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;
use crate::verify::contracts::store::load_freeze;
use crate::verify::integrity::{current_events, IntegrityEvent};
use crate::verify::review::store::open_findings;
use crate::verify::transitions::{load_stage, update_stage};

pub(crate) fn handle_file_dispute(
    work_dir: &Path,
    stage_id: &str,
    kind: DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
) -> Result<Response> {
    // The stage id arrives unvalidated from the wire and names directories.
    if let Err(error) = crate::validation::validate_id(stage_id) {
        return Ok(refused(format!("invalid stage_id: {error}")));
    }
    let locked = lock_stage_disputes(work_dir, stage_id)?;
    let stage = load_stage(stage_id, &locked.work_dir)?;
    let kind = match confirm(&locked.work_dir, &stage, kind) {
        Ok(kind) => kind,
        Err(error) => return Ok(refused(format!("{error:#}"))),
    };
    if dispute_budgets::exhausted(&stage, &kind) {
        return Ok(escalate_exhausted(&locked.work_dir, &stage, &kind));
    }

    let record = DisputeRequest {
        id: 0,
        stage_id: stage_id.to_string(),
        kind: kind.clone(),
        reason: reason.clone(),
        evidence_commit,
        failure_output: None,
        fix_attempts_at_dispute: stage.fix_attempts,
        created_at: Utc::now(),
    };
    let id = write_request(&locked.stage_dir, record)?;

    // Spend the budget and transition on the fresh on-disk stage, as the
    // criterion path does (A-5).
    update_stage(stage_id, &locked.work_dir, |s| {
        dispute_budgets::spend(s, &kind);
        s.tally.evidence_rounds = 0;
        s.try_request_adjudication(Some(reason))
    })?;
    drop(locked);
    Ok(Response::DisputeCreated { id })
}

fn refused(message: String) -> Response {
    Response::Error { message }
}

/// Escalate to NeedsHumanReview and refuse, as the criterion path does when
/// its budget is spent.
fn escalate_exhausted(work_dir: &Path, stage: &Stage, kind: &DisputeKind) -> Response {
    let name = kind.name();
    escalate_to_human_review(&stage.id, work_dir, |fresh| {
        format!(
            "Dispute budget exhausted ({} of {MAX_DISPUTES_PER_KIND} {name} disputes filed)",
            dispute_budgets::filed(fresh, kind)
        )
    });
    refused(format!(
        "Dispute budget exhausted ({} {name} disputes filed; max is {MAX_DISPUTES_PER_KIND}).",
        dispute_budgets::filed(stage, kind)
    ))
}

/// The dispute to record: `kind` with every id checked against what exists
/// now and its evidence re-derived by the daemon.
fn confirm(work_dir: &Path, stage: &Stage, kind: DisputeKind) -> Result<DisputeKind> {
    if stage.plan_version != 2 {
        bail!(
            "stage '{}' is not a plan version 2 stage; only its acceptance criteria can be \
             disputed, with `loom stage dispute-criteria`",
            stage.id
        );
    }
    match kind {
        DisputeKind::Criterion { .. } => {
            bail!("a criterion is disputed with `loom stage dispute-criteria`")
        }
        DisputeKind::Findings { finding_ids, .. } => {
            let open = open_findings(work_dir, &stage.id)?;
            let evidence = select_findings(&open, &finding_ids)?;
            Ok(DisputeKind::Findings {
                finding_ids,
                evidence,
            })
        }
        DisputeKind::Contract { contract_id } => {
            check_frozen(work_dir, &stage.id, &contract_id)?;
            Ok(DisputeKind::Contract { contract_id })
        }
        DisputeKind::Integrity { event_ids, .. } => {
            let events = integrity_events_now(work_dir, stage)?;
            let evidence = select_events(&events, &event_ids)?;
            Ok(DisputeKind::Integrity {
                event_ids,
                evidence,
            })
        }
    }
}

fn check_frozen(work_dir: &Path, stage_id: &str, contract_id: &str) -> Result<()> {
    let Some(record) = load_freeze(work_dir, stage_id)? else {
        bail!("the contracts of stage '{stage_id}' are not frozen");
    };
    if !record
        .contracts
        .iter()
        .any(|frozen| frozen.id == contract_id)
    {
        bail!("'{contract_id}' names no frozen contract of this stage");
    }
    Ok(())
}

/// The stage's test-integrity events now: its worktree scanned against the
/// configured target branch, the base the completion gate uses.
fn integrity_events_now(work_dir: &Path, stage: &Stage) -> Result<Vec<IntegrityEvent>> {
    let workspace = WorkDir::new(work_dir)?;
    let repo_root = workspace
        .repo_root()
        .context("cannot resolve the repository root of the state directory")?;
    let worktree_id = stage.worktree.as_deref().unwrap_or(&stage.id);
    crate::validation::validate_id(worktree_id).context("invalid worktree id")?;
    let worktree = Worktree::worktree_path(repo_root, worktree_id)
        .canonicalize()
        .with_context(|| format!("stage '{}' has no worktree", stage.id))?;
    let target = crate::fs::resolve_target_branch_from_config(work_dir, repo_root)?;
    current_events(&worktree, &target, &stage.ratchet_files)
}

#[cfg(test)]
#[path = "dispute_kinds_tests.rs"]
mod tests;
