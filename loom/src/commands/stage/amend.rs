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

/// `loom stage amend` — repair a stage's `acceptance` or `wiring` array.
///
/// `field`/`patch` are already-validated domain types built by the caller
/// from the CLI's `--field`/`--op`/`--index`/`--value` flags; this function
/// only resolves the plan path, fills in a default `reason`, and forwards
/// to [`apply_amendment`].
pub fn amend(
    stage_id: String,
    field: AmendmentField,
    patch: AmendmentPatch,
    reason: Option<String>,
) -> Result<()> {
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
