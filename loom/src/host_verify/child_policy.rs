//! Policy binding between the daemon's deadline calculation and its child.

use std::path::Path;

use anyhow::{ensure, Result};

use super::child;
use super::{JobResult, BUDGET_POLICY_ENV};
use crate::models::stage::Stage;

/// Refuse changed verification inputs before the child starts any checks.
pub(crate) fn run_job_bound(
    work_dir: &Path,
    stage_id: &str,
    job_id: &str,
    expected_policy: &str,
) -> Result<JobResult> {
    child::load_running(work_dir, stage_id, job_id)?;
    let stage = crate::verify::transitions::load_stage(stage_id, work_dir)?;
    let actual = crate::handoff::completion::policy::effective_policy_hash(&stage, work_dir)?;
    ensure!(
        expected_policy == actual,
        "verification policy changed after the host deadline was calculated"
    );
    child::run_job(work_dir, stage_id, job_id)
}

/// Recheck after a pipeline loads its own stage, before it executes commands.
pub(super) fn ensure_bound_stage(stage: &Stage, work_dir: &Path) -> Result<()> {
    if let Ok(expected) = std::env::var(BUDGET_POLICY_ENV) {
        let actual = crate::handoff::completion::policy::effective_policy_hash(stage, work_dir)?;
        ensure!(
            expected == actual,
            "verification policy changed before the host job executed"
        );
    }
    Ok(())
}
