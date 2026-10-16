//! RPC adapter for the shared stage application service.
use crate::daemon::protocol::Response;
use anyhow::Result;
use std::path::Path;
pub(crate) fn handle_freeze_contracts(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
    reports: &[crate::stage_service::ContractRunReport],
) -> Result<Response> {
    crate::stage_service::handle_freeze_contracts(work_dir, stage_id, session_id, reports)
        .map(Into::into)
}
