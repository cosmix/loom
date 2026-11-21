//! The host child: one host job, run by `loom stage verify-host`.
//!
//! The daemon's runner starts it for a job it has set `Running`. The child verifies the tree the
//! request named, records each criterion's outcome as it finishes, and writes the job's result. A
//! check runs the stage's gate, its contracts (`child_contracts`) or criteria at its base
//! (`child_base`), a completion or a verdict completion its full run (`child_complete`), a freeze
//! its contracts (`freeze`). The CLI wrapper prints the result summary; checks may print progress
//! to `child.log`.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, ensure, Context, Result};

use super::child_policy::{check_cache_policy, requested_cache};
use super::child_progress::acceptance_label;
pub(super) use super::child_progress::Progress;
use super::{child_base, child_complete, child_contracts, digest, freeze, store};
use super::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::git::worktree::WorktreeGit;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::{Stage, StageType};
use crate::verify::criteria::{
    confined_stage_cache, memory_journals_grant, plan_confinement, resolve_confinement,
    run_acceptance_with_config, set_spawn_observer, CachePolicy, CriteriaConfig, CriterionObserver,
    CriterionResult, MemoryJournals, SpawnObserver, SpawnObserverGuard,
};
use crate::verify::execution_paths::{resolve_acceptance_dir, StageExecutionPaths};
use crate::verify::goal_backward::plan::{self, LoadedGoal};
use crate::verify::goal_backward::GoalBackwardResult;
use crate::verify::transitions::load_stage;

/// Most bytes of one outcome's summary; the full output is in its log.
pub(super) const SUMMARY_MAX_BYTES: usize = 8 * 1024;
/// Most bytes of a result's pipeline error.
const ERROR_MAX_BYTES: usize = 2 * 1024;

/// Run the stage's current job `job_id`, which must be `Running`, and write
/// its result. A job that is not the current running one is an error that
/// writes nothing; any later failure is written as the result's `error`.
pub fn run_job(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<JobResult> {
    let record = load_running(work_dir, stage_id, job_id)?;
    let _watchdog = super::child_watchdog::start(work_dir, stage_id, job_id);
    let _groups = record_groups(work_dir, stage_id, job_id);
    let result = match record.kind {
        JobKind::Check => {
            let progress = Arc::new(Progress::new(work_dir, stage_id, job_id));
            let checked = check(work_dir, &record, &progress);
            finish(&record, progress.take_outcomes(), checked, None)
        }
        JobKind::Complete | JobKind::VerdictComplete => child_complete::run(work_dir, &record),
        JobKind::Freeze => freeze::run_freeze_job(work_dir, &record)
            .unwrap_or_else(|error| finish(&record, Vec::new(), Err(error), None)),
    };
    store::save_result(work_dir, stage_id, &result)?;
    Ok(result)
}

pub(super) fn load_running(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<JobRecord> {
    let record = store::load_current(work_dir, stage_id)?
        .with_context(|| format!("stage '{stage_id}' has no current host job"))?;
    ensure!(
        record.id == job_id,
        "host job {job_id} is not the current job of stage '{stage_id}' ({} is)",
        record.id
    );
    ensure!(
        record.state == JobState::Running,
        "host job {job_id} is {:?}, not running",
        record.state
    );
    Ok(record)
}

/// Append the process group of every confined command this process spawns
/// to the job's `groups` file, so retiring the job reaches its criteria.
/// The observer stays installed until the returned guard drops.
fn record_groups(work_dir: &Path, stage_id: &str, job_id: &str) -> SpawnObserverGuard {
    let work_dir = work_dir.to_path_buf();
    let (stage_id, job_id) = (stage_id.to_string(), job_id.to_string());
    set_spawn_observer(SpawnObserver(Box::new(move || {
        let (child, address) = store::start_guarded_group(&work_dir, &stage_id, &job_id)?;
        Ok(crate::verify::criteria::GroupAnchor::new(
            child,
            Box::new(move || address.signal()),
        ))
    })))
}

/// The check pipeline. `Ok(Some(change))` names how the tree changed while
/// the criteria ran.
fn check(work_dir: &Path, record: &JobRecord, progress: &Arc<Progress>) -> Result<Option<String>> {
    let stage = load_stage(&record.stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{}'", record.stage_id))?;
    super::child_policy::ensure_bound_stage(&stage, work_dir)?;
    let expected = std::env::var(super::BUDGET_POLICY_ENV).ok();
    let goal = plan::load(&stage.id, work_dir, expected.as_deref())?;
    let tree = JobTree::resolve(&stage, work_dir)?;
    if record.at_base {
        return child_base::run(work_dir, record, &stage, &tree, progress);
    }
    if let Some(mismatch) = tree.differences(record)? {
        bail!("the tree does not match the request: {mismatch}");
    }
    if record.contracts {
        return child_contracts::run(work_dir, record, &stage, &tree, progress);
    }
    let only = record.criteria.clone().map(BTreeSet::from_iter);
    let cache = check_cache_policy(work_dir, record)?;
    let root = tree.paths.worktree_root.as_deref();
    let config = acceptance_config(&stage, root, work_dir, cache, progress)?
        .with_memory_journals(tree.memory_journals.clone());
    run_configured(&stage, &tree, &config.with_only(only))?;
    progress.take_failure()?;
    // The goal-backward checks and the contracts belong to a run of every criterion.
    if record.criteria.is_none() {
        if goal.has_checks() {
            run_goal(&stage, &tree, progress, &goal)?;
        }
        child_contracts::run_full(&stage, &tree, work_dir, progress)?;
        progress.take_failure()?;
        if let Some(integrity) = crate::daemon::check_integrity_gate(work_dir, &stage) {
            let outcome = child_complete::check_outcome(INTEGRITY_LABEL, &integrity);
            progress.record(outcome, None);
        }
    }
    tree.differences(record)
}

/// The outcome label of the test-integrity gate a full check runs.
pub(super) const INTEGRITY_LABEL: &str = "test-integrity";

/// Run every acceptance criterion, as a completion does. A check builds the
/// same configuration, so a completion reuses each pass a check on the same
/// tree cached.
pub(super) fn run_acceptance(
    stage: &Stage,
    tree: &JobTree,
    work_dir: &Path,
    no_cache: bool,
    progress: &Arc<Progress>,
) -> Result<()> {
    let root = tree.paths.worktree_root.as_deref();
    let config = acceptance_config(stage, root, work_dir, requested_cache(no_cache), progress)?
        .with_memory_journals(tree.memory_journals.clone());
    run_configured(stage, tree, &config)
}

/// A job's acceptance configuration: each criterion's output reaches its log
/// as it runs and the criterion is recorded as it finishes, the record decides
/// caching, never the daemon's environment, and confined commands share the
/// stage's build caches.
pub(super) fn acceptance_config(
    stage: &Stage,
    tree_root: Option<&Path>,
    work_dir: &Path,
    cache: CachePolicy,
    progress: &Arc<Progress>,
) -> Result<CriteriaConfig> {
    let observer = {
        let progress = Arc::clone(progress);
        let setup = stage.setup.clone();
        CriterionObserver(Arc::new(move |index: usize, result: &CriterionResult| {
            progress.acceptance(index, result, &setup);
        }))
    };
    let plan = plan_confinement(work_dir);
    let confinement = resolve_confinement(stage.sandbox.command_confinement, plan);
    let stage_cache = confined_stage_cache(work_dir, &stage.id, confinement)?;
    Ok(CriteriaConfig::default()
        .with_plan_confinement(plan)
        .with_tree_root(tree_root)
        .with_stage_cache(stage_cache)
        .with_observer(observer)
        .with_live_output(progress.live_output(acceptance_label))
        .with_cache_dir(work_dir)
        .with_cache_policy(cache))
}

fn run_configured(stage: &Stage, tree: &JobTree, config: &CriteriaConfig) -> Result<()> {
    run_acceptance_with_config(stage, tree.paths.acceptance_dir.as_deref(), config)
        .context("Failed to run acceptance criteria")?;
    Ok(())
}

fn run_goal(stage: &Stage, tree: &JobTree, progress: &Progress, goal: &LoadedGoal) -> Result<()> {
    let verify_dir = tree
        .paths
        .acceptance_dir
        .as_deref()
        .context("No working directory available for goal-backward verification")?;
    let result = goal.run(
        verify_dir,
        tree.paths.worktree_root.as_deref(),
        stage.plan_version,
    )?;
    let outcome = CriterionOutcome {
        label: "goal-backward".to_string(),
        passed: result.is_passed(),
        summary: truncate_to_byte_limit(&goal_summary(&result), SUMMARY_MAX_BYTES),
        log: None,
    };
    progress.record(outcome, None);
    Ok(())
}

/// The gap list `loom check` prints for a goal-backward result, uncoloured.
fn goal_summary(result: &GoalBackwardResult) -> String {
    match result {
        GoalBackwardResult::Passed => "  ✓ All artifacts exist\n  ✓ All wiring connected".into(),
        GoalBackwardResult::GapsFound { gaps } => {
            let mut lines = vec![format!("  ✗ Found {} gap(s):", gaps.len())];
            for gap in gaps {
                lines.push(format!("    → {}", gap.description));
                lines.push(format!("      Fix: {}", gap.suggestion));
            }
            lines.join("\n")
        }
        GoalBackwardResult::HumanNeeded { checks } => {
            let mut lines = vec!["  ? Manual review needed:".to_string()];
            lines.extend(checks.iter().map(|check| format!("    {check}")));
            lines.join("\n")
        }
    }
}

/// The job's result from its outcomes and pipeline end; `Ok(Some(change))` names a tree change.
pub(super) fn finish(
    record: &JobRecord,
    criteria: Vec<CriterionOutcome>,
    checked: Result<Option<String>>,
    policy_hash: Option<String>,
) -> JobResult {
    let error = match checked {
        Ok(None) => None,
        Ok(Some(change)) => Some(format!("worktree changed during the run: {change}")),
        Err(err) => Some(format!("{err:#}")),
    }
    .map(|error| truncate_to_byte_limit(&error, ERROR_MAX_BYTES));
    let passed = error.is_none() && criteria.iter().all(|outcome| outcome.passed);
    JobResult {
        job_id: record.id.clone(),
        head: record.head.clone(),
        passed,
        criteria,
        error,
        policy_hash,
    }
}

/// The checkout a job verifies and where its criteria run: the stage
/// worktree, or the repository root for a knowledge stage.
pub(super) struct JobTree {
    /// Git in the checkout. This process runs outside every sandbox, so a
    /// stage worktree's git is pinned to its registered git directory: the
    /// worktree's `.git` file cannot choose the configuration git runs. A
    /// knowledge stage's checkout is the repository root, whose git directory
    /// every daemon git call trusts.
    pub(super) git: WorktreeGit,
    pub(super) paths: StageExecutionPaths,
    pub(super) knowledge: bool,
    /// The memory journals a knowledge-distill stage's criteria read beside
    /// its worktree, from the daemon's state root ([`memory_journals_grant`]).
    pub(super) memory_journals: Option<MemoryJournals>,
}

impl JobTree {
    /// Resolved from the state root, never from the current directory. A
    /// knowledge stage's paths are those
    /// `acceptance_runner::resolve_knowledge_execution_paths` gives a process
    /// started in the repository root: no `worktree_root`, so a `working_dir`
    /// below the root does not grant the whole repository.
    pub(super) fn resolve(stage: &Stage, work_dir: &Path) -> Result<Self> {
        let repo_root = crate::fs::work_dir::WorkDir::new(work_dir)?
            .main_project_root()
            .context("the repository of the state root is unavailable")?;
        let knowledge = stage.stage_type == StageType::Knowledge;
        let (checkout, worktree_root) = if knowledge {
            (repo_root.clone(), None)
        } else {
            let root = crate::git::worktree::stage_worktree_path(stage, &repo_root)?;
            (root.clone(), Some(root))
        };
        ensure!(
            checkout.is_dir(),
            "the tree of stage '{}' is missing: {}",
            stage.id,
            checkout.display()
        );
        let acceptance_dir = resolve_acceptance_dir(Some(&checkout), stage.working_dir.as_deref())?;
        let memory_journals = memory_journals_grant(stage, work_dir)?;
        if knowledge {
            // The criteria's confinement holds the state root and hides each
            // secret by binding over the file (`confine_protected`), so the
            // attestation key, else made on the first completion, exists first.
            crate::handoff::attestation_key(work_dir)?;
        }
        let git = match &worktree_root {
            Some(root) => WorktreeGit::pinned(&repo_root, root)?,
            None => WorktreeGit::discovered(&checkout),
        };
        Ok(Self {
            git,
            paths: StageExecutionPaths {
                worktree_root,
                acceptance_dir,
            },
            knowledge,
            memory_journals,
        })
    }

    /// How the checkout's HEAD and digest differ from the record's, if they
    /// do. A clean commit leaves the digest unchanged, so HEAD is compared on
    /// its own.
    pub(super) fn differences(&self, record: &JobRecord) -> Result<Option<String>> {
        let head = digest::head_in(&self.git)?;
        let digest = if self.knowledge {
            digest::knowledge_tree_digest_in(&self.git)?
        } else {
            digest::tree_digest_in(&self.git)?
        };
        let mut found = Vec::new();
        if head != record.head {
            found.push(format!("HEAD is {head}, the request named {}", record.head));
        }
        if digest != record.tree_digest {
            found.push(format!(
                "tree digest is {digest}, the request saw {}",
                record.tree_digest
            ));
        }
        Ok((!found.is_empty()).then(|| found.join("; ")))
    }
}
