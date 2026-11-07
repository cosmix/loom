//! Adjudication subsystem.
//!
//! Disputes filed by agents land in `.loom/work/disputes/<stage>/<n>/request.md`.
//! The orchestrator polls these files every tick:
//!
//! * [`AdjudicatorRegistry::disputes_awaiting_session`] returns the disputes
//!   that need an adjudication session started, having applied every guard
//!   (a verdict already written, a stage that has left `NeedsAdjudication`,
//!   the evidence-round cap, a session already live for the stage, the
//!   spawn-attempt cap), plus an [`Escalation`] for every stage a guard hands
//!   to a human. The daemon spawns the sessions, and retires each escalated
//!   stage's disputing agent before writing its escalation, which closes the
//!   stage's open disputes (`closed_disputes.rs`); see
//!   `orchestrator/core/orchestrator.rs`.
//! * [`AdjudicatorRegistry::apply_pending_verdicts`] scans for verdict
//!   files that haven't been applied (no `applied.marker`) and mutates
//!   stage state accordingly (see `apply.rs`, and `apply_kinds.rs` for the
//!   routing by dispute kind).
//!
//! The adjudicator is a real loom session, not a subprocess the daemon waits
//! on: it is spawned into a terminal inside the disputed stage's worktree when
//! that worktree still exists (the main repository otherwise), judges the
//! dispute with the full tool surface, and hands its verdict over by running
//! `loom stage adjudicate` (see [`record`]). The daemon never blocks on it — it
//! observes `verdict.md` appearing on a later tick, exactly as merge
//! resolution observes `loom stage merge --resolved`.
//!
//! The registry therefore holds no state at all: liveness comes from the
//! session record and the spawn budget from the dispute directory, so a
//! daemon restart mid-adjudication neither loses a running session nor
//! resets its budget.

mod apply;
mod apply_accept;
mod apply_contract;
mod apply_freeze_refusal;
mod apply_impact;
mod apply_kinds;
mod apply_receipt;
mod closed_disputes;
mod counter;
mod escalation;
pub mod feedback;
mod plan_patch;
pub mod prompt;
pub mod record;
mod requeue;
mod scan;
pub mod session;
pub mod verdict;
mod verdict_freeze_refusal;
mod verdict_kinds;

#[cfg(test)]
mod apply_contract_tests;
#[cfg(test)]
mod apply_kinds_tests;
#[cfg(test)]
mod closed_disputes_tests;
#[cfg(test)]
mod escalation_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_criterion_field;
#[cfg(test)]
mod tests_retry_budget;
#[cfg(test)]
mod tests_verdicts;
#[cfg(test)]
mod verdict_hold_contract_tests;
#[cfg(test)]
mod verdict_hold_tests;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::models::dispute::{dispute_dir, request_file, verdict_file};
use crate::models::stage::StageStatus;
use crate::stage_service::dispute_is_orphaned;
use crate::verify::transitions::load_stage;

use closed_disputes::is_closed;
use scan::{read_dispute_request, scan_pending_requests};

pub(crate) use closed_disputes::{close_open_disputes_except_locked, lock_stage_dispute_dir};
#[cfg(test)]
pub(crate) use closed_disputes::{close_open_disputes_locked, fail_close_once};
pub use escalation::Escalation;
pub(crate) use requeue::{hand_back_held_stage, is_held_for_completion, is_verdict_held};
pub(crate) use scan::dispute_ids;
pub use session::{
    attempt_count, persist_verdict, read_request, resolve_model, scratch_verdict_draft,
    scratch_verdict_file_name, verdict_draft_file, AdjudicationJob, DEFAULT_ADJUDICATION_MODEL,
    MAX_ADJUDICATION_ATTEMPTS,
};

/// Maximum evidence-loop rounds. After this, the stage escalates to
/// `NeedsHumanReview` instead of looping forever.
///
/// Five, not three: each round is a real exchange — the adjudicator asks
/// specific questions, the stage agent answers them in its next attempt — and
/// three cut that conversation off while it was still converging. The loop is
/// bounded because an adjudicator that cannot decide after five rounds of its
/// own questions is not going to decide on the sixth, not because rounds are
/// expensive.
pub const MAX_EVIDENCE_ROUNDS: u32 = 5;

/// Apply attempts a single dispute's verdict may fail before the daemon gives
/// up and escalates instead of retrying forever.
///
/// A verdict that still cannot apply after this many ticks is a wedge, not a
/// transient failure a retry would clear — a malformed `plan_patch`, a
/// corrupt `verdict.md`, or a stage file `load_stage` cannot parse all fail
/// identically on every attempt. Left uncapped, every failed attempt also
/// re-triggers `retire_disputing_agents` before the next one (see
/// `orchestrator/core/verdict_apply.rs`), which kills any new session on the
/// stage within seconds of it starting.
pub const MAX_APPLY_ATTEMPTS: u32 = 3;

/// The daemon's entry points into the dispute lifecycle.
///
/// Deliberately stateless — see the module docs. It is owned by the
/// [`Orchestrator`](crate::orchestrator::Orchestrator) and lives for the
/// entire daemon run.
#[derive(Debug, Default, Clone, Copy)]
pub struct AdjudicatorRegistry;

impl AdjudicatorRegistry {
    pub fn new() -> Self {
        Self
    }

    /// Disputes that need an adjudication session started on this tick, and
    /// the stages a terminal guard hands to a human instead.
    ///
    /// Every returned job has already been counted against the dispute's
    /// spawn budget: the caller is expected to try the spawn immediately, and
    /// [`AdjudicatorRegistry::start_pending_adjudications`] adds an escalation
    /// for the stage when it cannot. No escalation is written here; see
    /// [`Escalation`].
    pub fn disputes_awaiting_session(&self, work_dir: &Path) -> Result<PendingDisputes> {
        let disputes_root = work_dir.join("disputes");
        let mut pending = PendingDisputes::default();
        if !disputes_root.exists() {
            return Ok(pending);
        }
        for (stage_id, dispute_id) in scan_pending_requests(&disputes_root)? {
            // One outcome per stage per pass. A stage can carry more than one
            // unanswered dispute (a second one filed while the first awaits
            // its verdict), and the live-session guard below only sees
            // sessions started on an EARLIER tick — so without this, both
            // would be handed a session at once and two adjudicators
            // would judge the same stage in the same repository. A stage
            // escalated on this pass is still `NeedsAdjudication` until the
            // caller writes it, so it is skipped here too.
            if pending.covers(&stage_id) {
                continue;
            }
            if let Some(job) =
                self.job_for_dispute(work_dir, &stage_id, dispute_id, &mut pending.escalations)?
            {
                pending.jobs.push(job);
            }
        }
        Ok(pending)
    }

    /// The guards, in order, for one pending dispute. `None` means "no session
    /// this tick" — either because the dispute is already handled, or because
    /// a terminal condition pushed an escalation onto `escalations` instead.
    fn job_for_dispute(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
        escalations: &mut Vec<Escalation>,
    ) -> Result<Option<AdjudicationJob>> {
        let disputes_root = work_dir.join("disputes");
        if verdict_file(&disputes_root, stage_id, dispute_id).exists() {
            return Ok(None);
        }
        let request = read_dispute_request(&request_file(&disputes_root, stage_id, dispute_id))
            .map_err(|error| {
                tracing::warn!(target: "loom::adjudication", stage = %stage_id, dispute = dispute_id, %error, "skipping unparseable dispute request");
            })
            .ok();
        let Some(request) = request else {
            return Ok(None);
        };

        let stage = load_stage(stage_id, work_dir)
            .map_err(|error| {
                tracing::warn!(target: "loom::adjudication", stage = %stage_id, %error, "could not load stage; skipping dispute");
            })
            .ok();
        let Some(stage) = stage else { return Ok(None) };
        if stage.status != StageStatus::NeedsAdjudication {
            return Ok(None);
        }
        if dispute_is_orphaned(&stage, dispute_id) {
            settle_orphaned_request(work_dir, stage_id, dispute_id)?;
            return Ok(None);
        }
        if stage.tally.evidence_rounds >= MAX_EVIDENCE_ROUNDS {
            escalations.push(Escalation::evidence_cap(stage_id));
            return Ok(None);
        }
        if !self.claim_session_slot(work_dir, stage_id, dispute_id, escalations)? {
            return Ok(None);
        }

        Ok(Some(AdjudicationJob {
            stage,
            request,
            plan_path: resolve_plan_path(work_dir).unwrap_or_else(|| PathBuf::from("PLAN.md")),
        }))
    }
}

impl AdjudicatorRegistry {
    /// Whether a new adjudication session may be started for this dispute,
    /// spending one of its attempts if so.
    ///
    /// `false` means either that a session is already judging the stage — the
    /// reason not to start a second one, and one that holds across daemon
    /// restarts because it is answered from the session record — or that the
    /// dispute has used its budget, in which case an escalation is pushed onto
    /// `escalations` so the stage does not wait on a session that will never
    /// come.
    fn claim_session_slot(
        &self,
        work_dir: &Path,
        stage_id: &str,
        dispute_id: u32,
        escalations: &mut Vec<Escalation>,
    ) -> Result<bool> {
        match session::live_adjudication_session(work_dir, stage_id) {
            Ok(Some(session_id)) => {
                tracing::debug!(target: "loom::adjudication", stage = %stage_id, session = %session_id, "adjudication session already live");
                return Ok(false);
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(target: "loom::adjudication", stage = %stage_id, %error,
                    "Session ownership is uncertain; deferring adjudication spawn");
                return Ok(false);
            }
        }
        let attempts = session::checked_attempt_count(work_dir, stage_id, dispute_id)?;
        if attempts >= MAX_ADJUDICATION_ATTEMPTS {
            escalations.push(Escalation::attempt_cap(stage_id, dispute_id, attempts));
            return Ok(false);
        }
        session::record_attempt(work_dir, stage_id, dispute_id)?;
        Ok(true)
    }
}

/// Close a request whose filing crashed before the stage write, so the budget
/// unit its refiling spent pays for one judgement and the request holds no
/// verdict's re-queue. A filing holds the dispute lock across both its writes,
/// so the stage read under it has named that filing's request, unless the
/// filing is gone.
fn settle_orphaned_request(work_dir: &Path, stage_id: &str, dispute_id: u32) -> Result<()> {
    let _lifecycle = lock_stage_dispute_dir(&work_dir.join("disputes").join(stage_id))?;
    let stage = load_stage(stage_id, work_dir)?;
    if stage.status == StageStatus::NeedsAdjudication && dispute_is_orphaned(&stage, dispute_id) {
        tracing::warn!(target: "loom::adjudication", stage = %stage_id, dispute = dispute_id,
            "closing a dispute request no filing published");
        closed_disputes::close_dispute_locked(work_dir, stage_id, dispute_id)?;
    }
    Ok(())
}

/// What one pass over `.loom/work/disputes/` found to act on.
#[derive(Debug, Default)]
pub struct PendingDisputes {
    /// Disputes to start an adjudication session for, at most one per stage.
    pub jobs: Vec<AdjudicationJob>,
    /// Stages a terminal guard hands to a human, not yet written.
    pub escalations: Vec<Escalation>,
}

impl PendingDisputes {
    /// Whether this pass already has an outcome for `stage_id`.
    fn covers(&self, stage_id: &str) -> bool {
        self.jobs.iter().any(|job| job.stage.id == stage_id)
            || self
                .escalations
                .iter()
                .any(|escalation| escalation.stage_id == stage_id)
    }
}

/// Escalate a dispute whose verdict has now failed to apply
/// [`MAX_APPLY_ATTEMPTS`] times.
///
/// Written immediately, unlike the dispute loop's escalations:
/// `Orchestrator::apply_pending_verdicts` retires the disputing agent before
/// every apply attempt, so by the time an attempt fails the agent is already
/// gone and `stage.session` released.
fn escalate_apply_cap_claimed(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    failures: u32,
    error: &anyhow::Error,
    claim: &std::os::fd::OwnedFd,
) -> Result<()> {
    anyhow::ensure!(
        Escalation::apply_cap(stage_id, dispute_id, failures, error).write_claimed(
            work_dir,
            claim,
            Some((apply_receipt::cap_receipt(dispute_id), dispute_id))
        )?,
        "stage left adjudication before apply-cap escalation"
    );
    Ok(())
}

/// Historical file name for the per-dispute apply-attempt counter.
/// A charge precedes each apply so a failed counter write cannot allow a
/// free retry. The name remains stable across daemon restarts.
const APPLY_FAILURES_FILENAME: &str = "apply_failures";

fn apply_failures_file(work_dir: &Path, stage_id: &str, dispute_id: u32) -> PathBuf {
    dispute_dir(&work_dir.join("disputes"), stage_id, dispute_id).join(APPLY_FAILURES_FILENAME)
}

/// How many apply attempts this dispute's verdict has already consumed.
fn apply_failure_count(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
) -> std::result::Result<u32, counter::CounterError> {
    counter::read(&apply_failures_file(work_dir, stage_id, dispute_id))
}

/// Reserve one apply attempt durably and return the new total.
///
fn reserve_apply_attempt(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
) -> std::result::Result<u32, counter::CounterError> {
    let path = apply_failures_file(work_dir, stage_id, dispute_id);
    counter::charge(&path)
}

/// The stage's worktree, located the way the freeze handler
/// (`stage_service/contracts.rs`) locates it: git pinned to its registered
/// git directory, since the daemon reads a worktree the disputing agent wrote.
fn locate_stage_site(
    work_dir: &Path,
    stage: &crate::models::stage::Stage,
) -> Result<crate::stage_service::contracts::Site> {
    let work_dir = work_dir
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", work_dir.display()))?;
    crate::stage_service::contracts::locate(&work_dir, stage)
}

/// The plan's `source_path`, resolved against the main project root that
/// `.loom/work` belongs to (two levels up from the work dir, followed through
/// a worktree's symlink).
fn resolve_plan_path(work_dir: &Path) -> Option<PathBuf> {
    crate::fs::resolve_source_path(work_dir).ok().flatten()
}
