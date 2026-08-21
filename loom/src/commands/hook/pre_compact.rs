//! `loom hook pre-compact` — reopen delivery suppression for a compacting session.
//!
//! `hooks/pre-compact.sh` already runs a block-then-allow handoff protocol on
//! every PreCompact event (see that script's own header); this delegate rides
//! alongside it, deleting exactly one delivery record so retrieval stops
//! assuming the compacting session still holds what it was already given.
//!
//! Suppression in [`crate::context::delivery`] is scoped to a recipient and an
//! epoch: while a session's context window is intact, skipping units it
//! already read is correct. A compaction breaks that assumption — the
//! summarized context that survives compaction may drop the brief entirely —
//! so the moment compaction happens is the moment this session's own record
//! must stop suppressing anything. It is NOT a moment to touch any *other*
//! record: a live sibling session's suppression, and the stage's own spawn
//! record, both describe context windows this compaction never touched.
//!
//! Same fail-open contract as every hook delegate in this module (see
//! `super::user_prompt`): malformed or absent stdin, an environment naming
//! neither a stage nor a checkout, or a filesystem error all read as "nothing
//! to reset" rather than a failure. A PreCompact hook that errors or prints
//! disrupts compaction, and a delivery record is an optimisation nothing here
//! may treat as load-bearing (`crate::context::delivery`'s own module doc).

use crate::context::delivery;
use crate::context::local_overlay::local_overlay_key;
use crate::fs::work_dir::WorkDir;
use crate::validation::validate_id;
use anyhow::Result;
use std::io::Read;
use std::path::PathBuf;

/// Longest stdin payload worth parsing — the same reasoning as
/// `super::user_prompt::MAX_STDIN_BYTES`: the shell side owns the timeout,
/// this is only what keeps a pathological payload bounded in memory.
const MAX_STDIN_BYTES: u64 = 1024 * 1024;

/// Reset the compacting session's own delivery suppression, or do nothing.
///
/// Always returns `Ok(())`: there is no failure mode a PreCompact hook is
/// allowed to surface, only cases where there was nothing honest to reset.
pub fn pre_compact() -> Result<()> {
    let mut raw = String::new();
    let _ = std::io::stdin()
        .lock()
        .take(MAX_STDIN_BYTES)
        .read_to_string(&mut raw);
    reset_for_payload(&raw);
    Ok(())
}

/// Testable core: given the hook payload's raw bytes (already read from
/// stdin) and the process environment, reset the compacting session's own
/// delivery record, or do nothing. Split out so tests can drive it with a
/// literal JSON string instead of real stdin, the same shape
/// `super::user_prompt::retrieve_for_prompt` uses for its own core.
fn reset_for_payload(raw: &str) {
    let Some(session_id) = parse_session_id(raw) else {
        return;
    };
    let Some(target) = CompactionTarget::from_environment() else {
        return;
    };
    target.reset(&session_id);
}

/// The `session_id` field from a hook payload, or `None` for anything that is
/// not "a JSON object naming a non-blank session id" — the same discipline
/// `super::user_prompt::parse_prompt` applies to its own field.
fn parse_session_id(raw: &str) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_str(raw).ok()?;
    let session_id = payload.get("session_id")?.as_str()?.trim();
    (!session_id.is_empty()).then(|| session_id.to_string())
}

/// Where a compaction resets suppression: the same `(work_dir, plan,
/// stage_id)` address a prompt hook resolves in this same environment
/// (`super::user_prompt::DeliveryTarget`). Re-derived here rather than
/// imported — that type is private to its own module, and neither module has
/// a shared resolver to import from today (worth lifting into one later; see
/// this stage's report).
struct CompactionTarget {
    work_dir: PathBuf,
    plan: String,
    stage_id: String,
}

impl CompactionTarget {
    fn from_environment() -> Option<Self> {
        Self::for_stage().or_else(Self::for_checkout)
    }

    /// `LOOM_STAGE_ID` + `LOOM_WORK_DIR`, when both are set and name a real
    /// stage — the same preference and the same validate-at-the-boundary
    /// discipline `super::user_prompt::DeliveryTarget::for_stage` uses,
    /// because the stage id here becomes a path component by way of
    /// [`delivery::hook_recipient_id`].
    fn for_stage() -> Option<Self> {
        let stage_id = non_empty_env("LOOM_STAGE_ID")?;
        validate_id(&stage_id).ok()?;
        let work_dir = WorkDir::new(non_empty_env("LOOM_WORK_DIR")?).ok()?;
        let stage = crate::verify::load_stage(&stage_id, work_dir.root()).ok()?;
        Some(CompactionTarget {
            work_dir: work_dir.root().to_path_buf(),
            plan: delivery::plan_key(&stage).to_string(),
            stage_id,
        })
    }

    /// The checkout this session is running in, for a compaction no stage
    /// claims — the same address [`local_overlay_key`] resolves for an
    /// ordinary Claude Code session.
    fn for_checkout() -> Option<Self> {
        let hint = non_empty_env("LOOM_WORK_DIR").unwrap_or_else(|| ".".to_string());
        let work_dir = WorkDir::new(hint).ok()?;
        let project_root = work_dir.project_root()?.to_path_buf();
        let (plan, stage_id) = local_overlay_key(&project_root);
        Some(CompactionTarget {
            work_dir: work_dir.root().to_path_buf(),
            plan,
            stage_id,
        })
    }

    /// Delete this session's own delivery record. A failure here is not
    /// reported anywhere beyond a debug log: the whole point of this call is
    /// best-effort cleanup, and a hook that surfaces its own bookkeeping
    /// errors is a hook that can disrupt compaction over a filesystem hiccup.
    fn reset(&self, session_id: &str) {
        let recipient = delivery::hook_recipient_id(&self.stage_id, Some(session_id));
        if let Err(error) = delivery::discard_session_delivery(
            &self.work_dir,
            &self.plan,
            &self.stage_id,
            &recipient,
        ) {
            tracing::debug!(%error, "Could not reset a compacted session's delivery record");
        }
    }
}

/// A set environment variable with non-blank content — the same helper
/// `super::user_prompt` keeps for itself; three lines duplicated rather than
/// shared, since neither hook delegate otherwise imports from the other.
fn non_empty_env(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    (!value.trim().is_empty()).then_some(value)
}

#[cfg(test)]
#[path = "tests_pre_compact.rs"]
mod tests;
