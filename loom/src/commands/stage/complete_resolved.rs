//! Operator adapter for trusted resolver completion checks.

use anyhow::Result;
use std::path::Path;

use crate::models::stage::Stage;

pub(crate) fn verify_resolved_completion(stage: &Stage, work_dir: &Path) -> Result<Stage> {
    crate::verify::completion::resolved::verify_resolved_completion(
        stage,
        work_dir,
        super::complete_verification::print_event,
    )
    .inspect_err(|error| {
        let message = format!("{error:#}");
        if message.contains("Acceptance criteria failed")
            || message.contains("Goal-backward verification failed")
        {
            crate::commands::stage::acceptance_runner::print_acceptance_failure_guidance(&stage.id);
        }
    })
}
