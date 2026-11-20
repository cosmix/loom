//! Operator-facing wrapper over the runtime plan-amendment path.
//!
//! `apply_amendment` (see `crate::plan::amendment`) already implements
//! everything needed to repair a broken acceptance/wiring criterion on an
//! existing stage — atomically, validated against the real
//! [`AcceptanceCriterion`](crate::plan::schema::AcceptanceCriterion) /
//! [`WiringCheck`](crate::models::stage::WiringCheck) types, with a durable
//! audit trail. Its only caller used to be the (agent-driven) adjudication
//! path, which requires a running daemon and a dispute verdict. This module
//! gives an operator the same route for a stage that is not currently
//! executing. It performs no mutation of its own — the caller
//! (`cli::dispatch`) has already turned the clap-level `--field`/`--op`
//! flags into the real [`AmendmentField`]/[`AmendmentPatch`] types, and
//! every write here goes through `apply_amendment`.

use anyhow::{bail, Context, Result};

use crate::plan::amendment::is_amendment_cap_error;
use crate::plan::{apply_amendment, AmendmentField, AmendmentPatch, AmendmentRequest};

const DEFAULT_REASON: &str = "operator amendment via loom stage amend";

/// Refuse `loom stage amend` from a stage worktree session.
///
/// A pure function of the worktree path the caller observed, so the rule is
/// testable without mutating the process environment: an absent (or blank)
/// path is the only acceptable state. A stage that could amend its own plan
/// could rewrite the checks it is judged by.
///
/// This is a guard against a session that runs the command by mistake, not a
/// security boundary: a session can unset the variables it is checked by. The
/// boundary is the session's write denies on the state directory (`.loom`),
/// which stop `apply_amendment` from writing whatever the environment says.
pub fn refuse_stage_session(worktree_path: Option<&str>) -> Result<()> {
    let Some(path) = worktree_path.map(str::trim).filter(|p| !p.is_empty()) else {
        return Ok(());
    };
    bail!(
        "This session runs inside the stage worktree at {path}, so it cannot amend the plan: a \
         stage may not change its own checks. `loom stage amend` is an operator command; \
         dispute a wrong check instead."
    )
}

/// Refuse when the session identity variables (`LOOM_STAGE_ID`,
/// `LOOM_SESSION_ID`) are set: a session that unset `LOOM_WORKTREE_PATH` still
/// carries them. Blank values name no session.
pub fn refuse_session_identity(stage_id: Option<&str>, session_id: Option<&str>) -> Result<()> {
    let named = |value: Option<&str>| value.is_some_and(|v| !v.trim().is_empty());
    if named(stage_id) || named(session_id) {
        bail!(
            "This process runs inside a loom session (LOOM_STAGE_ID or LOOM_SESSION_ID is set), \
             so it cannot amend the plan: a stage may not change its own checks. `loom stage \
             amend` is an operator command; dispute a wrong check instead."
        );
    }
    Ok(())
}

/// `loom stage amend` — repair one of a stage's `acceptance`, `wiring`,
/// `wiring_tests`, `contracts`, `harness` or `reachable` arrays.
///
/// `field`/`patch` are already-validated domain types built by the caller
/// from the CLI's `--field`/`--op`/`--index`/`--value` flags; this function
/// refuses a stage worktree session ([`refuse_stage_session`]), resolves the
/// plan path, fills in a default `reason`, and forwards to [`apply_amendment`].
pub fn amend(
    stage_id: String,
    field: AmendmentField,
    patch: AmendmentPatch,
    reason: Option<String>,
) -> Result<()> {
    refuse_stage_session(std::env::var("LOOM_WORKTREE_PATH").ok().as_deref())?;
    refuse_session_identity(
        std::env::var("LOOM_STAGE_ID").ok().as_deref(),
        std::env::var("LOOM_SESSION_ID").ok().as_deref(),
    )?;
    let reason = reason.unwrap_or_else(|| DEFAULT_REASON.to_string());

    let work_dir = crate::commands::common::work_dir_path()?;
    let plan_path = crate::fs::resolve_source_path(&work_dir)
        .context("Failed to resolve plan source_path from config.toml")?
        .ok_or_else(|| anyhow::anyhow!("No plan source_path configured in config.toml"))?;

    let request = AmendmentRequest {
        stage_id: stage_id.clone(),
        field,
        patch,
        reason: Some(reason),
        dispute_id: None,
    };

    match apply_amendment(&plan_path, &work_dir, request) {
        Ok(result) => {
            println!(
                "Amended stage '{}' ({:?}) -- version {}, amendments_applied {}",
                result.stage_id, result.field, result.version, result.amendments_applied
            );
            println!("Snapshot: {}", result.snapshot_path.display());
            Ok(())
        }
        Err(err) if is_amendment_cap_error(&err) => {
            bail!(
                "Amendment cap reached for stage '{stage_id}' -- too many amendments already \
                 applied. Escalate for manual review instead of amending again: {err}"
            )
        }
        Err(err) => Err(err).with_context(|| format!("Failed to amend stage '{stage_id}'")),
    }
}

#[cfg(test)]
#[path = "amend_identity_tests.rs"]
mod identity_tests;
