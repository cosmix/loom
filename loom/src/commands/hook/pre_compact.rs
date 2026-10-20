//! `loom hook pre-compact` — reopen delivery suppression for a compacted session.
//!
//! Two hooks run this delegate. `loom-hooks/pre-compact.sh` runs it on every
//! PreCompact event of a stage session, before its block-then-allow handoff.
//! `loom-hooks/knowledge-orient.sh` runs it for an ordinary session on the
//! SessionStart event whose `source` is `compact`, which fires once after a
//! compaction. No PreCompact hook is registered outside stage worktrees
//! (`fs::permissions::hooks::config::build`).
//!
//! Suppression in [`crate::context::delivery`] is scoped to a recipient and an
//! epoch: while a session's context window is intact, skipping units it
//! already read is correct. A compaction may drop what the session was given,
//! so the delegate deletes exactly one record, the compacting session's own
//! prompt-hook record. A live sibling session's record stays.
//!
//! A stage session's spawn record also stays, so a compacted stage session
//! still counts its spawn brief as delivered. Stage sessions have not
//! compacted in practice, and `loom-hooks/session-start.sh` tells a compacted
//! one to re-read its signal.
//!
//! Same fail-open contract as every hook delegate in this module (see
//! `super::user_prompt`): malformed or absent stdin, an environment naming
//! neither a stage nor a checkout, or a filesystem error all read as "nothing
//! to reset" rather than a failure. A PreCompact hook that errors or prints
//! disrupts compaction, and a delivery record is an optimisation nothing here
//! may treat as load-bearing (`crate::context::delivery`'s own module doc).

use crate::context::delivery;
use anyhow::Result;
use std::io::Read;

use super::target::HookTarget;

/// Longest stdin payload worth parsing — the same reasoning as
/// `super::user_prompt::MAX_STDIN_BYTES`: the shell side owns the timeout,
/// this is only what keeps a pathological payload bounded in memory.
const MAX_STDIN_BYTES: u64 = 1024 * 1024;

/// Reset the compacting session's own delivery suppression, or do nothing.
///
/// Always returns `Ok(())`: there is no failure mode a PreCompact hook is
/// allowed to surface, only cases where there was nothing honest to reset.
pub fn pre_compact() -> Result<()> {
    pre_compact_from(std::io::stdin().lock())
}

/// Entry point with injected input reader, so tests never read real stdin.
///
/// Reads at most `MAX_STDIN_BYTES` from `input`, parses the session ID,
/// and resets the delivery record if the environment names a valid target.
fn pre_compact_from(input: impl Read) -> Result<()> {
    let mut raw = String::new();
    let _ = input.take(MAX_STDIN_BYTES).read_to_string(&mut raw);
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
    let Some(target) = HookTarget::from_environment() else {
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

impl HookTarget {
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

#[cfg(test)]
#[path = "tests_pre_compact.rs"]
mod tests;
