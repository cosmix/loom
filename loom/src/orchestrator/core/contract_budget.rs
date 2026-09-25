//! The contract writer's respawn budget (DESIGN D8).
//!
//! A contract writer that ends without freezing is replaced, up to
//! [`MAX_CONTRACT_RESPAWNS`] times per stage; then the stage waits for a
//! human. Two paths hand out such a replacement and both charge it here: the
//! `ContractSessionEnded` handler while a daemon watches the writer, and
//! orphan recovery for a writer that died while no daemon did.

use std::path::Path;

use anyhow::Result;

use crate::models::session::{Session, SessionType};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::contracts::store::{attempts_spent, load_freeze, spend_attempt};

/// Fresh contract writers a stage may be handed after its first one ended
/// without freezing. Spent when handed out.
pub(super) const MAX_CONTRACT_RESPAWNS: u32 = 3;

/// Charge one replacement contract writer to the stage's budget and return
/// the number spent so far, or `None`, charging nothing, once
/// [`MAX_CONTRACT_RESPAWNS`] replacements have been handed out.
pub(super) fn charge_contract_respawn(work_dir: &Path, stage_id: &str) -> Result<Option<u32>> {
    if attempts_spent(work_dir, stage_id)? >= MAX_CONTRACT_RESPAWNS {
        return Ok(None);
    }
    spend_attempt(work_dir, stage_id).map(Some)
}

/// End the contract phase of an `Executing` stage whose budget is spent:
/// release its writer and wait for a human.
pub(super) fn request_contract_review(stage: &mut Stage) -> Result<()> {
    stage.try_request_human_review(format!(
        "contract session ended {MAX_CONTRACT_RESPAWNS} times without freezing contracts"
    ))?;
    stage.release_session();
    Ok(())
}

/// Orphan recovery's charge for `stage`, whose current `session` was found
/// dead after a daemon restart. Returns `true` when the budget was spent and
/// the stage went to `NeedsHumanReview`; `false` leaves the stage to the
/// generic requeue.
///
/// A contract writer of an `Executing` stage that died with nothing frozen
/// is what `ContractSessionEnded` reports under a watching daemon, and the
/// requeue that follows hands out a fresh writer, so it is charged here.
/// Nothing else is: with a freeze record the next spawn is the implementer;
/// a writer that handed off at its context ceiling (`NeedsHandoff`) is
/// continued uncharged under a watching daemon too; and a `Blocked` stage
/// raises no `ContractSessionEnded` and has no edge to `NeedsHumanReview`.
///
/// `manual_mode` also leaves the budget alone: a manually-launched writer
/// carries no PID-identity or window evidence loom wrote itself (loom never
/// spawned it), so the liveness probe reports it dead unconditionally, and
/// every `loom run --manual` restart would otherwise burn one respawn on a
/// writer that may still be alive in the operator's own terminal. The stage
/// still requeues normally; only the charge is skipped.
pub(super) fn charge_orphaned_contract_writer(
    stage: &mut Stage,
    session: &Session,
    work_dir: &Path,
    manual_mode: bool,
) -> Result<bool> {
    let replaced = session.session_type == SessionType::Contract
        && stage.status == StageStatus::Executing
        && load_freeze(work_dir, &stage.id)?.is_none();
    if !replaced {
        return Ok(false);
    }
    if manual_mode {
        tracing::debug!(
            stage_id = %stage.id,
            session_id = %session.id,
            "Manual mode: not charging an apparently-orphaned contract session to the respawn \
             budget; loom cannot prove liveness of a writer it did not launch"
        );
        return Ok(false);
    }
    if let Some(attempt) = charge_contract_respawn(work_dir, &stage.id)? {
        tracing::info!(
            stage_id = %stage.id,
            session_id = %session.id,
            attempt,
            "Charged the replacement of an orphaned contract session to the respawn budget"
        );
        return Ok(false);
    }
    request_contract_review(stage)?;
    Ok(true)
}

#[cfg(test)]
#[path = "contract_budget_tests.rs"]
mod tests;
