//! Who may use the privileged completion flags, and when a proof is required.
//!
//! Split from `complete` because this is the security decision, and it should
//! be readable — and reviewable — without the merge routing and acceptance
//! machinery around it.

use anyhow::Result;
use std::path::Path;

use crate::commands::stage::admin_proof::{
    admin_credential_exists, verify_and_consume_admin_proof, AdminProofRequest,
};

/// Verify and consume the caller-supplied proof for an exact completion request.
pub fn require_admin_capability(
    work_dir: &Path,
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    proof: Option<&str>,
) -> Result<()> {
    let request = AdminProofRequest::completion(stage_id, no_verify, force_unsafe, assume_merged);
    verify_and_consume_admin_proof(work_dir, request, proof)
}

pub(super) fn authorize_privileged_completion(
    stage_id: &str,
    no_verify: bool,
    force_unsafe: bool,
    assume_merged: bool,
    proof: Option<&str>,
    work_dir: &Path,
) -> Result<()> {
    if !(no_verify || force_unsafe || assume_merged) {
        return Ok(());
    }
    // No daemon means no credential for ANYONE (`admin.token` lives only while
    // the daemon runs), and its absence removes a stage agent's ability to ACT
    // — no broker, and `.work/**` is denyWrite — so demanding a proof would
    // lock out only the operator. See `mistakes/sandbox-and-settings.md`.
    if proof.is_none() && !admin_credential_exists(work_dir) {
        return Ok(());
    }
    require_admin_capability(
        work_dir,
        stage_id,
        no_verify,
        force_unsafe,
        assume_merged,
        proof,
    )
}

#[cfg(test)]
#[path = "tests/complete_authorization.rs"]
mod tests;
