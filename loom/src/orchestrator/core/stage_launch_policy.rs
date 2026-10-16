//! Final launch-time revalidation, after pure snapshot candidate selection.
use super::{
    stage_executor::{StageExecutor, StartDisposition},
    Orchestrator,
};
use crate::models::failure::FailureType;
use crate::models::stage::Stage;
use crate::orchestrator::scheduling_report::BlockReason;
use anyhow::Result;

impl Orchestrator {
    pub(super) fn spawn_dependencies_satisfied(&mut self, stage: &Stage) -> bool {
        let target = crate::git::branch::resolve_target_branch(
            &self.config.base_branch,
            &self.config.repo_root,
        );
        let check = crate::verify::transitions::are_all_dependencies_satisfied_cached(
            stage,
            &self.config.work_dir,
            &self.config.repo_root,
            &target,
        );
        let reason = match check {
            Ok(true) => return true,
            Ok(false) => match crate::verify::transitions::describe_dependency_block(
                stage,
                &self.config.work_dir,
                &self.config.repo_root,
                &target,
            ) {
                Ok(Some(block)) => BlockReason::Dependency {
                    dependency: block.dependency,
                    detail: block.detail,
                    self_resolving: block.self_resolving,
                },
                other => BlockReason::DependencyCheckFailed {
                    detail: format!("Dependency state changed or could not be checked: {other:?}"),
                },
            },
            Err(error) => BlockReason::DependencyCheckFailed {
                detail: error.to_string(),
            },
        };
        if self.spawn_skip_logged.insert(stage.id.clone()) {
            tracing::error!(stage_id = %stage.id, reason = %reason.describe(), "Refusing stage spawn");
        }
        self.spawn_blocks.insert(stage.id.clone(), reason);
        false
    }

    pub(super) fn start_guarded_knowledge(&mut self, stage: Stage) -> Result<StartDisposition> {
        let id = stage.id.clone();
        if let Err(error) = self.start_knowledge_stage(stage) {
            let detail = format!("{error:#}");
            if self
                .persist_blocked_stage(&id, FailureType::InfrastructureError, vec![detail])
                .is_ok()
            {
                let _ = self
                    .graph
                    .mark_status(&id, crate::models::stage::StageStatus::Blocked);
            }
        }
        Ok(if self.active_sessions.contains_key(&id) {
            StartDisposition::Launched
        } else {
            StartDisposition::Deferred
        })
    }
}
