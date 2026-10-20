//! RPC adapter for the shared stage application service.
use crate::daemon::protocol::Response;
use anyhow::Result;
use std::path::Path;
pub(crate) fn handle_block_stage(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
) -> Result<Response> {
    crate::stage_service::handle_block_stage(work_dir, stage_id, reason).map(Into::into)
}
pub(crate) fn handle_agent_block_stage(
    work_dir: &Path,
    stage_id: &str,
    reason: &str,
) -> Result<Response> {
    crate::stage_service::handle_agent_block_stage(work_dir, stage_id, reason).map(Into::into)
}
