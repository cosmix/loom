//! RPC adapter for the shared stage application service.
use crate::daemon::protocol::Response;
use anyhow::Result;
use std::path::Path;

/// Queue the host contract run of `stage_id` for its contract writer; see
/// [`crate::stage_service::handle_freeze_contracts`].
pub(crate) fn handle_freeze_contracts(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    head: &str,
    tree_digest: &str,
    request_id: &str,
) -> Result<Response> {
    crate::stage_service::handle_freeze_contracts(
        work_dir,
        stage_id,
        session_id,
        head,
        tree_digest,
        request_id,
    )
    .map(Into::into)
}
