//! Policy binding between the daemon's deadline calculation and its child.

use std::path::Path;

use anyhow::{ensure, Result};

use super::child;
use super::{JobRecord, JobResult, BUDGET_POLICY_ENV};
use crate::fs::session_files::load_session_exact;
use crate::models::session::SessionType;
use crate::models::stage::Stage;
use crate::verify::criteria::CachePolicy;

/// `--no-cache` reads nothing but stores what it re-proves.
pub(super) fn requested_cache(no_cache: bool) -> CachePolicy {
    if no_cache {
        CachePolicy::Refresh
    } else {
        CachePolicy::Use
    }
}

/// A merge resolver's check neither reads nor stores the pass cache. The cache
/// keys on content, so a stored pass would answer the Complete job of the
/// session a handback starts, and `land_stage_merge` runs no gate of its own.
pub(super) fn check_cache_policy(work_dir: &Path, record: &JobRecord) -> Result<CachePolicy> {
    let session = load_session_exact(work_dir, &record.session_id)?;
    if session.is_some_and(|session| session.session_type == SessionType::Merge) {
        return Ok(CachePolicy::Bypass);
    }
    Ok(requested_cache(record.no_cache))
}

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
