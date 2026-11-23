//! Sections a recovery signal shares with a fresh one, built by the same formatters.

use std::path::Path;

use crate::models::stage::Stage;

use super::format::{
    format_criterion, format_dependency_sections, format_goal_backward_verification_section,
    format_semi_stable_section,
};
use super::types::{DependencyStatus, EmbeddedContext};

/// Push the semi-stable section: brief, memory reminder, codex lane, response
/// budget, ultracode license and sandbox. The gates come from `stage`, as in a
/// fresh signal, so a caller's context need not carry them.
pub(super) fn push_semi_stable(
    signal: &mut String,
    stage: &Stage,
    embedded_context: &EmbeddedContext,
) {
    let mut context = embedded_context.clone();
    context.implementers = stage.implementers.clone();
    context.subagent_timeout_secs = stage.subagent_timeout_secs;
    context.ultracode = stage.ultracode;
    signal.push_str(&format_semi_stable_section(
        &context,
        stage.stage_type,
        &stage.id,
    ));
}

/// Push the assignment, acceptance criteria, goal-backward checks and files.
pub(super) fn push_recovery_assignment(signal: &mut String, stage: &Stage) {
    signal.push_str("## Assignment\n\n");
    signal.push_str(&format!("{}\n\n", stage.name));
    if let Some(ref desc) = stage.description {
        signal.push_str(&format!("{desc}\n\n"));
    }

    if !stage.acceptance.is_empty() {
        signal.push_str("## Acceptance Criteria\n\n");
        for criterion in &stage.acceptance {
            signal.push_str(&format_criterion(criterion));
        }
        signal.push('\n');
    }
    if stage.has_any_goal_checks() {
        signal.push_str(&format_goal_backward_verification_section(stage));
    }

    if !stage.files.is_empty() {
        signal.push_str("## Files to Modify\n\n");
        for file in &stage.files {
            signal.push_str(&format!("- {file}\n"));
        }
        signal.push('\n');
    }
}

/// Each dependency's saved status and outputs; an unreadable stage file reads as
/// `Unknown` so a damaged dependency cannot fail the recovery.
fn recovery_dependency_status(stage: &Stage, work_dir: &Path) -> Vec<DependencyStatus> {
    stage
        .dependencies
        .iter()
        .map(|id| {
            let (status, outputs) = match crate::verify::transitions::load_stage(id, work_dir) {
                Ok(dep) => (format!("{:?}", dep.status), dep.outputs),
                Err(_) => ("Unknown".to_string(), Vec::new()),
            };
            DependencyStatus {
                stage_id: id.clone(),
                name: id.clone(),
                status,
                outputs,
            }
        })
        .collect()
}

/// The `## Dependencies Status` and `## Dependency Outputs` sections of `stage`.
pub(super) fn format_recovery_dependencies(stage: &Stage, work_dir: &Path) -> String {
    format_dependency_sections(&recovery_dependency_status(stage, work_dir))
}
