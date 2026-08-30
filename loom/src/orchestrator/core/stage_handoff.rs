//! Safe handoff selection during stage spawn.

use crate::handoff::find_continuation_handoff_name;
use crate::models::failure::FailureType;

use super::Orchestrator;

impl Orchestrator {
    /// Resolve the exact predecessor's handoff, or contain lookup uncertainty
    /// as a pre-spawn infrastructure failure.
    ///
    /// The outer `Option` says whether spawning may continue; the inner one is
    /// the legitimate "this predecessor wrote no handoff" result.
    pub(super) fn continuation_handoff_or_block(
        &mut self,
        stage_id: &str,
        outgoing_session_id: Option<&str>,
        incoming_session_id: &str,
    ) -> Option<Option<String>> {
        match find_continuation_handoff_name(stage_id, outgoing_session_id, &self.config.work_dir) {
            Ok(handoff) => Some(handoff),
            Err(error) => {
                self.block_and_undo_session(
                    stage_id,
                    incoming_session_id,
                    FailureType::InfrastructureError,
                    format!("Failed to select continuation handoff: {error:#}"),
                );
                None
            }
        }
    }
}
