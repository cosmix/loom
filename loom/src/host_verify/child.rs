//! The host child: one host job, run by `loom stage verify-host`.
//!
//! The daemon's runner starts it for a job it has set `Running`. The child
//! verifies the tree the request named, records each criterion's outcome as
//! it finishes, and writes the job's result. A check runs the stage's gate, a
//! completion its full completion run (`child_complete`), a freeze its
//! contracts (`freeze`). It prints nothing; the CLI wrapper prints the
//! summary the runner captures in `child.log`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use anyhow::{bail, ensure, Context, Result};

use super::{child_complete, digest, freeze, store};
use super::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::commands::stage::acceptance_runner::{resolve_acceptance_dir, StageExecutionPaths};
use crate::commands::stage::criterion_output::format_criterion_result;
use crate::commands::verify::run_and_verify_stage_goal;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::{Stage, StageType};
use crate::verify::criteria::{
    plan_confinement, run_acceptance_with_config, set_spawn_observer, CachePolicy, CriteriaConfig,
    CriterionObserver, CriterionResult, SpawnObserver, SpawnObserverGuard,
};
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
    let _groups = record_groups(work_dir, stage_id, job_id);
    let result = match record.kind {
        JobKind::Check => {
            let progress = Arc::new(Progress::new(work_dir, stage_id, job_id));
            let checked = check(work_dir, &record, &progress);
            finish(&record, progress.take_outcomes(), checked, None)
        }
        JobKind::Complete => child_complete::run(work_dir, &record),
        JobKind::Freeze => freeze::run_freeze_job(work_dir, &record)
            .unwrap_or_else(|error| finish(&record, Vec::new(), Err(error), None)),
    };
    store::save_result(work_dir, stage_id, &result)?;
    Ok(result)
}

fn load_running(work_dir: &Path, stage_id: &str, job_id: &str) -> Result<JobRecord> {
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
    set_spawn_observer(SpawnObserver(Box::new(move |pgid| {
        if let Err(err) = store::append_group(&work_dir, &stage_id, &job_id, pgid) {
            tracing::warn!("host job {job_id}: cannot record process group {pgid}: {err:#}");
        }
    })))
}

/// The check pipeline. `Ok(Some(change))` names how the tree changed while
/// the criteria ran.
fn check(work_dir: &Path, record: &JobRecord, progress: &Arc<Progress>) -> Result<Option<String>> {
    let stage = load_stage(&record.stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{}'", record.stage_id))?;
    let goal_checks = plan_has_goal_checks(work_dir, &stage.id)?;
    let tree = JobTree::resolve(&stage, work_dir)?;
    if let Some(mismatch) = tree.differences(record)? {
        bail!("the tree does not match the request: {mismatch}");
    }
    run_acceptance(&stage, &tree, work_dir, record.no_cache, progress)?;
    progress.take_failure()?;
    if goal_checks {
        run_goal(&stage, &tree, work_dir, progress)?;
        progress.take_failure()?;
    }
    tree.differences(record)
}

/// Whether the stage's plan entry defines goal checks. The plan is the state
/// root's `config.toml` source; without one the job cannot know, so it fails.
fn plan_has_goal_checks(work_dir: &Path, stage_id: &str) -> Result<bool> {
    let plan_path = crate::fs::resolve_source_path(work_dir)?
        .context("No plan source path configured in config.toml")?;
    let plan = crate::plan::parse_plan(&plan_path)
        .with_context(|| format!("Failed to parse plan: {}", plan_path.display()))?;
    let definition = plan
        .stages
        .iter()
        .find(|definition| definition.id == stage_id)
        .with_context(|| format!("Stage '{stage_id}' not found in plan"))?;
    Ok(definition.has_any_goal_checks())
}

pub(super) fn run_acceptance(
    stage: &Stage,
    tree: &JobTree,
    work_dir: &Path,
    no_cache: bool,
    progress: &Arc<Progress>,
) -> Result<()> {
    let observer = {
        let progress = Arc::clone(progress);
        let setup = stage.setup.clone();
        CriterionObserver(Arc::new(move |index: usize, result: &CriterionResult| {
            progress.acceptance(index, result, &setup);
        }))
    };
    let mut config = CriteriaConfig::default()
        .with_plan_confinement(plan_confinement(work_dir))
        .with_tree_root(tree.paths.worktree_root.as_deref())
        .with_observer(observer);
    if !no_cache {
        // The record decides caching, never the daemon's environment.
        config = config
            .with_cache_dir(work_dir)
            .with_cache_policy(CachePolicy::Use);
    }
    run_acceptance_with_config(stage, tree.paths.acceptance_dir.as_deref(), &config)
        .context("Failed to run acceptance criteria")?;
    Ok(())
}

fn run_goal(stage: &Stage, tree: &JobTree, work_dir: &Path, progress: &Progress) -> Result<()> {
    let verify_dir = tree
        .paths
        .acceptance_dir
        .as_deref()
        .context("No working directory available for goal-backward verification")?;
    let result = run_and_verify_stage_goal(
        &stage.id,
        verify_dir,
        tree.paths.worktree_root.as_deref(),
        work_dir,
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

/// The job's result from its outcomes and how its pipeline ended:
/// `Ok(Some(change))` names how the tree changed during the run.
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
    checkout: PathBuf,
    pub(super) paths: StageExecutionPaths,
    pub(super) knowledge: bool,
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
            (repo_root, None)
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
        Ok(Self {
            checkout,
            paths: StageExecutionPaths {
                worktree_root,
                acceptance_dir,
            },
            knowledge,
        })
    }

    /// How the checkout's HEAD and digest differ from the record's, if they
    /// do. A clean commit leaves the digest unchanged, so HEAD is compared on
    /// its own.
    pub(super) fn differences(&self, record: &JobRecord) -> Result<Option<String>> {
        let head = digest::head(&self.checkout)?;
        let digest = if self.knowledge {
            digest::knowledge_tree_digest(&self.checkout)?
        } else {
            digest::tree_digest(&self.checkout)?
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

/// The outcomes of one job, persisted as each arrives. The acceptance
/// observer cannot return an error, so the first store failure is kept and
/// turned into the job's error once the criteria finish.
pub(super) struct Progress {
    work_dir: PathBuf,
    stage_id: String,
    job_id: String,
    state: Mutex<ProgressState>,
}

#[derive(Default)]
struct ProgressState {
    outcomes: Vec<CriterionOutcome>,
    failure: Option<String>,
}

impl Progress {
    pub(super) fn new(work_dir: &Path, stage_id: &str, job_id: &str) -> Self {
        Self {
            work_dir: work_dir.to_path_buf(),
            stage_id: stage_id.to_string(),
            job_id: job_id.to_string(),
            state: Mutex::new(ProgressState::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, ProgressState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record acceptance criterion `index`. A result answered from the cache
    /// captured no output and gets no log.
    fn acceptance(&self, index: usize, result: &CriterionResult, setup: &[String]) {
        let summary = format_criterion_result(index, result, setup);
        let outcome = CriterionOutcome {
            label: format!("acceptance-{index}"),
            passed: result.success,
            summary: truncate_to_byte_limit(&summary, SUMMARY_MAX_BYTES),
            log: None,
        };
        let output =
            (!result.cached).then(|| [result.stdout.as_bytes(), result.stderr.as_bytes()].concat());
        self.record(outcome, output);
    }

    pub(super) fn record(&self, mut outcome: CriterionOutcome, output: Option<Vec<u8>>) {
        let stored = self.store(&mut outcome, output);
        let mut state = self.lock();
        if let Err(err) = stored {
            state.failure.get_or_insert_with(|| format!("{err:#}"));
        }
        state.outcomes.push(outcome);
    }

    fn store(&self, outcome: &mut CriterionOutcome, output: Option<Vec<u8>>) -> Result<()> {
        let (work_dir, stage_id, job_id) = (&self.work_dir, &self.stage_id, &self.job_id);
        if let Some(bytes) = output {
            let log =
                store::save_criterion_log(work_dir, stage_id, job_id, &outcome.label, &bytes)?;
            outcome.log = Some(log);
        }
        store::append_progress(work_dir, stage_id, job_id, outcome)
    }

    pub(super) fn take_failure(&self) -> Result<()> {
        match self.lock().failure.take() {
            Some(failure) => bail!("cannot record the job's progress: {failure}"),
            None => Ok(()),
        }
    }

    /// Whether any outcome recorded so far failed.
    pub(super) fn any_failed(&self) -> bool {
        self.lock().outcomes.iter().any(|outcome| !outcome.passed)
    }

    pub(super) fn take_outcomes(&self) -> Vec<CriterionOutcome> {
        std::mem::take(&mut self.lock().outcomes)
    }
}
