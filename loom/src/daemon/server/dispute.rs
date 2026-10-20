//! RPC adapter for the shared stage application service.
use crate::daemon::protocol::Response;
use anyhow::Result;
use std::path::Path;
pub fn handle_dispute_criteria(
    work_dir: &Path,
    stage_id: &str,
    field: crate::models::dispute::CriterionField,
    criterion_indices: Vec<usize>,
    reason: String,
    evidence_commit: Option<String>,
    failure_output: Option<String>,
) -> Result<Response> {
    crate::stage_service::handle_dispute_criteria(
        work_dir,
        stage_id,
        field,
        criterion_indices,
        reason,
        evidence_commit,
        failure_output,
    )
    .map(Into::into)
}
