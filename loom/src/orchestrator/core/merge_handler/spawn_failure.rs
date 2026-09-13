//! Reporting a merge-resolver spawn failure: log it, and route the stage to
//! human review when the spawn was refused at the sandbox boundary rather
//! than failing for an ordinary operational reason (owner decision 4).
//! Relocated out of `merge_handler.rs` to keep it within its
//! maintainability-ledger line budget — unrelated to the merge gate itself.

use crate::models::failure::FailureType;
use crate::orchestrator::core::{clear_status_line, spawn_failure_type, Orchestrator};

impl Orchestrator {
    /// Log a merge-resolver spawn failure; block only on a sandbox refusal (owner decision 4).
    pub(super) fn report_merge_spawn_failure(&mut self, stage_id: &str, e: anyhow::Error) {
        clear_status_line();
        eprintln!("Warning: Failed to spawn merge resolution session for '{stage_id}': {e}");
        if let Some(reason) = merge_spawn_block_reason(stage_id, &e) {
            self.route_to_human_review(stage_id, reason, Some(FailureType::SandboxSetupFailure));
        }
    }
}

/// `NeedsHumanReview` reason for a sandbox-refused merge-resolver spawn
/// (owner decision 4); `None` otherwise.
fn merge_spawn_block_reason(stage_id: &str, error: &anyhow::Error) -> Option<String> {
    if spawn_failure_type(error) != FailureType::SandboxSetupFailure {
        return None;
    }
    Some(format!(
        "sandbox preflight refused the merge resolver: {error:#}. Resolve manually with \
         `loom stage merge {stage_id}`."
    ))
}

#[cfg(test)]
mod tests {
    use super::merge_spawn_block_reason;
    use crate::sandbox::preflight::SandboxPreflightRefusal;

    #[test]
    fn merge_spawn_preflight_refusal_blocks_and_only_it_does() {
        let refusal = SandboxPreflightRefusal::new(vec!["LOOM_BIN lies under /repo".to_string()]);
        let error = anyhow::Error::new(refusal);
        assert!(merge_spawn_block_reason("s", &error).is_some());
        let other = anyhow::anyhow!("tmux could not create its socket directory");
        assert!(merge_spawn_block_reason("s", &other).is_none());
    }
}
