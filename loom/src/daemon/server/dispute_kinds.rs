//! RPC adapter for the shared stage application service.
use crate::daemon::protocol::Response;
use anyhow::Result;
use std::path::Path;
pub(crate) fn handle_file_dispute(
    work_dir: &Path,
    stage_id: &str,
    kind: crate::models::dispute::DisputeKind,
    reason: String,
    evidence_commit: Option<String>,
) -> Result<Response> {
    crate::stage_service::handle_file_dispute(work_dir, stage_id, kind, reason, evidence_commit)
        .map(Into::into)
}
