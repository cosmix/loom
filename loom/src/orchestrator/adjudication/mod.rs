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
mod apply_contract;
mod apply_kinds;
mod closed_disputes;
mod escalation;
pub mod feedback;
mod plan_patch;
pub mod prompt;
pub mod record;
mod requeue;
mod scan;
pub mod session;
pub mod verdict;
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
mod tests_verdicts;

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::models::dispute::{dispute_dir, request_file, verdict_file};
use crate::models::stage::StageStatus;
use crate::verify::transitions::load_stage;

use closed_disputes::is_closed;
use scan::{read_dispute_request, scan_pending_requests};

pub(crate) use closed_disputes::close_open_disputes;
pub use escalation::Escalation;
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
                self.job_for_dispute(work_dir, &stage_id, dispute_id, &mut pending.escalations)
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
    ) -> Option<AdjudicationJob> {
        let disputes_root = work_dir.join("disputes");
        if verdict_file(&disputes_root, stage_id, dispute_id).exists() {
            return None;
        }
        let request = read_dispute_request(&request_file(&disputes_root, stage_id, dispute_id))
            .map_err(|error| {
                tracing::warn!(target: "loom::adjudication", stage = %stage_id, dispute = dispute_id, %error, "skipping unparseable dispute request");
            })
            .ok()?;

        let stage = load_stage(stage_id, work_dir)
            .map_err(|error| {
                tracing::warn!(target: "loom::adjudication", stage = %stage_id, %error, "could not load stage; skipping dispute");
            })
            .ok()?;
        if stage.status != StageStatus::NeedsAdjudication {
            return None;
        }
        if stage.tally.evidence_rounds >= MAX_EVIDENCE_ROUNDS {
            escalations.push(Escalation::evidence_cap(stage_id));
            return None;
        }
        if !self.claim_session_slot(work_dir, stage_id, dispute_id, escalations) {
            return None;
        }

        Some(AdjudicationJob {
            stage,
            request,
            plan_path: resolve_plan_path(work_dir).unwrap_or_else(|| PathBuf::from("PLAN.md")),
        })
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
    ) -> bool {
        if let Some(session_id) = session::live_adjudication_session(work_dir, stage_id) {
            tracing::debug!(target: "loom::adjudication", stage = %stage_id, session = %session_id, "adjudication session already live");
            return false;
        }
        let attempts = session::attempt_count(work_dir, stage_id, dispute_id);
        if attempts >= MAX_ADJUDICATION_ATTEMPTS {
            escalations.push(Escalation::attempt_cap(stage_id, dispute_id, attempts));
            return false;
        }
        session::record_attempt(work_dir, stage_id, dispute_id);
        true
    }
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
fn escalate_apply_cap(
    work_dir: &Path,
    stage_id: &str,
    dispute_id: u32,
    failures: u32,
    error: &anyhow::Error,
) {
    Escalation::apply_cap(stage_id, dispute_id, failures, error).write(work_dir);
}

/// File name for the per-dispute apply-failure counter, mirroring
/// `session.rs`'s `attempts` file for adjudication spawn attempts.
const APPLY_FAILURES_FILENAME: &str = "apply_failures";

fn apply_failures_file(work_dir: &Path, stage_id: &str, dispute_id: u32) -> PathBuf {
    dispute_dir(&work_dir.join("disputes"), stage_id, dispute_id).join(APPLY_FAILURES_FILENAME)
}

/// How many times applying this dispute's verdict has already failed.
fn apply_failure_count(work_dir: &Path, stage_id: &str, dispute_id: u32) -> u32 {
    std::fs::read_to_string(apply_failures_file(work_dir, stage_id, dispute_id))
        .ok()
        .and_then(|raw| raw.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

/// Count one more apply failure and return the new total.
///
/// Best-effort like [`session::record_attempt`]: a directory that cannot be
/// created or written is warned about, never fatal — the caller still gets a
/// count to compare against [`MAX_APPLY_ATTEMPTS`] even when it could not be
/// persisted.
fn record_apply_failure(work_dir: &Path, stage_id: &str, dispute_id: u32) -> u32 {
    let path = apply_failures_file(work_dir, stage_id, dispute_id);
    let next = apply_failure_count(work_dir, stage_id, dispute_id).saturating_add(1);
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                target: "loom::adjudication",
                stage = %stage_id,
                dispute = dispute_id,
                %error,
                "could not create the dispute directory; apply failures are not being counted",
            );
            return next;
        }
    }
    if let Err(error) = std::fs::write(&path, next.to_string()) {
        tracing::warn!(
            target: "loom::adjudication",
            stage = %stage_id,
            dispute = dispute_id,
            %error,
            "could not persist the apply failure count",
        );
    }
    next
}

fn resolve_plan_path(work_dir: &Path) -> Option<PathBuf> {
    let cfg = crate::fs::work_dir::load_config(work_dir).ok().flatten()?;
    let path = cfg.source_path()?;
    if path.is_absolute() {
        Some(path)
    } else {
        let root = work_dir
            .canonicalize()
            .ok()
            .and_then(|wd| wd.parent().map(|p| p.to_path_buf()))?;
        Some(root.join(path))
    }
}
