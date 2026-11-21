//! A check at the stage's base (`loom check --criterion <n> --at-base`): the
//! selected criteria run, with the stage's setup, confinement and timeout,
//! against a checkout of the stage's base commit, the merge base the impact
//! tests use. The checkout and the run's build cache are kept beside the stage
//! cache, where the stage's own code cannot write (`stage_cache::base_checkout`),
//! so a second run reuses its build and the stage's caches never reach it.
//! The plan's provision entries run in the checkout, with that cache, before
//! any criterion does; a provision that fails stops the job with no evidence.
//! Each criterion becomes an outcome `base:<index>` and one [`BaseEvidence`],
//! naming the base, that a criterion dispute cites. The job reads the worktree
//! only to find the base, so it is not bound to the worktree's tree digest.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{anyhow, bail, ensure, Context, Result};

use super::child::{acceptance_config, JobTree, Progress, SUMMARY_MAX_BYTES};
use super::child_policy::requested_cache;
use super::submit::is_lower_hex;
use super::{CriterionOutcome, JobRecord};
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::Stage;
use crate::orchestrator::provision::read_provision_snapshot;
use crate::stage_cache::base_checkout::{ensure_base_dirs, BaseDirs};
use crate::stage_cache::StageCache;
use crate::testrun::{classify, registry, RunOutcome, RunOutput, RunSummary};
use crate::verify::context::CriteriaContext;
use crate::verify::contracts::changes::stage_base;
use crate::verify::criteria::{
    run_acceptance_with_config, running_block, stage_cache_for, CriteriaConfig, CriterionObserver,
    CriterionResult,
};
use crate::verify::execution_paths::resolve_acceptance_dir;
use crate::verify::impact_tests::base::{provision_base, BaseCheckout};
use crate::verify::review::handoffs::{save_base_evidence, BaseEvidence};

/// Hex digits of the base commit an outcome shows.
const SHORT_COMMIT: usize = 12;

/// Run `record`'s criteria at the stage's base and record each one.
pub(super) fn run(
    work_dir: &Path,
    record: &JobRecord,
    stage: &Stage,
    tree: &JobTree,
    progress: &Arc<Progress>,
) -> Result<Option<String>> {
    let Some(criteria) = record.criteria.clone() else {
        bail!("a check at the stage's base names the criteria it runs");
    };
    let worktree = tree
        .paths
        .worktree_root
        .as_deref()
        .context("a knowledge stage runs on the repository root and has no base")?;
    let base = stage_base(&tree.git, work_dir).context("the stage's base is unknown")?;
    ensure!(
        is_lower_hex(&base, &[40, 64]),
        "the stage's base {base:?} is no commit id"
    );
    let short = base[..SHORT_COMMIT].to_string();
    let dirs = base_dirs(work_dir, &stage.id, &base)?;
    let checkout = BaseCheckout::kept(&dirs.checkout, worktree, &base)
        .with_context(|| format!("the stage's base {short} could not be checked out"))?;
    announce(&short, &checkout);
    provision_kept_checkout(work_dir, &short, &checkout, &dirs.cache)?;
    let acceptance_dir =
        resolve_acceptance_dir(Some(checkout.root()), stage.working_dir.as_deref())?;
    let recorder = Arc::new(Recorder::new(
        work_dir,
        stage,
        base,
        acceptance_dir.clone(),
        progress,
    ));
    let config = base_config(stage, checkout.root(), &dirs.cache, work_dir, progress)?
        .with_observer(Recorder::observer(&recorder));
    run_acceptance_with_config(
        stage,
        acceptance_dir.as_deref(),
        &config.with_only(Some(criteria.into_iter().collect::<BTreeSet<_>>())),
    )
    .context("Failed to run acceptance criteria at the stage's base")?;
    progress.take_failure()?;
    recorder.take_failure()?;
    Ok(None)
}

/// The job's first progress line, in `child.log`.
fn announce(short: &str, checkout: &BaseCheckout) {
    let how = if checkout.reused() {
        "reusing the"
    } else {
        "made a new"
    };
    println!(
        "at base {short}: {how} checkout {}; it holds tracked files and what the plan's \
         provision installs, so a compiled project builds once per base and is warm afterwards",
        checkout.root().display()
    );
}

/// Run the plan's provision entries in the kept checkout, as the impact gate's
/// scratch base is provisioned, filling the base's own cache that the criteria
/// then read. A kept checkout is provisioned on every run: its installed
/// environment is ignored by git, so a reused one still holds what an earlier
/// run made, and an entry that finds it current is cheap. A failure stops the
/// job before any criterion runs, so no evidence of it is recorded.
fn provision_kept_checkout(
    work_dir: &Path,
    short: &str,
    checkout: &BaseCheckout,
    cache: &StageCache,
) -> Result<()> {
    let entries = read_provision_snapshot(work_dir)
        .context("the plan's provision snapshot in the state directory is unreadable")?;
    provision_base(&entries, checkout.root(), cache)
        .map_err(|reason| anyhow!("the stage's base {short} could not be provisioned: {reason}"))
}

/// The checkout and build cache of `base` beside stage `stage_id`'s cache,
/// their parents made and those of other bases removed.
pub(super) fn base_dirs(work_dir: &Path, stage_id: &str, base: &str) -> Result<BaseDirs> {
    ensure_base_dirs(&stage_cache_for(work_dir, stage_id)?, base)
}

/// A check's configuration with the checkout `root` as its tree, no pass
/// cache (a base run proves nothing about the worktree), and `cache`, the
/// base's own, in place of the stage cache a confined run would get: the
/// stage's code can write the stage cache, so a base run never reads it.
pub(super) fn base_config(
    stage: &Stage,
    root: &Path,
    cache: &StageCache,
    work_dir: &Path,
    progress: &Arc<Progress>,
) -> Result<CriteriaConfig> {
    let config = acceptance_config(stage, Some(root), work_dir, requested_cache(true), progress)?;
    let stage_cache = config.stage_cache.as_ref().map(|_| cache.clone());
    Ok(CriteriaConfig {
        cache_dir: None,
        stage_cache,
        live_output: Some(progress.live_output(base_label)),
        ..config
    })
}

/// The label, and log name, of criterion `index` run at the base.
fn base_label(index: usize) -> String {
    format!("base:{index}")
}

/// Records each criterion as it finishes: its outcome and log in the job,
/// its evidence in the stage's review records.
struct Recorder {
    work_dir: PathBuf,
    stage: Stage,
    base: String,
    acceptance_dir: Option<PathBuf>,
    progress: Arc<Progress>,
    failure: Mutex<Option<String>>,
}

impl Recorder {
    fn new(
        work_dir: &Path,
        stage: &Stage,
        base: String,
        acceptance_dir: Option<PathBuf>,
        progress: &Arc<Progress>,
    ) -> Self {
        Self {
            work_dir: work_dir.to_path_buf(),
            stage: stage.clone(),
            base,
            acceptance_dir,
            progress: Arc::clone(progress),
            failure: Mutex::new(None),
        }
    }

    fn record(&self, index: usize, result: &CriterionResult) {
        let Some(criterion) = self.stage.acceptance.get(index) else {
            return;
        };
        let cwd = self.acceptance_dir.as_deref().unwrap_or(Path::new("."));
        let expanded =
            CriteriaContext::with_stage_id(cwd, &self.stage.id).expand(criterion.command());
        let evidence = evidence(
            index,
            criterion.command(),
            &self.base,
            result,
            parsed(&expanded, cwd, result),
        );
        if let Err(error) = save_base_evidence(&self.work_dir, &self.stage.id, &evidence) {
            let mut failure = self.failure.lock().unwrap_or_else(PoisonError::into_inner);
            failure.get_or_insert_with(|| format!("{error:#}"));
        }
        let outcome = CriterionOutcome {
            label: base_label(index),
            passed: evidence.passed,
            summary: base_summary(&self.base[..SHORT_COMMIT], &evidence, result),
            log: None,
        };
        let output = [result.stdout.as_bytes(), result.stderr.as_bytes()].concat();
        self.progress.record(outcome, Some(output));
    }

    /// The observer that records each criterion through `recorder`.
    fn observer(recorder: &Arc<Recorder>) -> CriterionObserver {
        let recorder = Arc::clone(recorder);
        CriterionObserver(Arc::new(move |index: usize, result: &CriterionResult| {
            recorder.record(index, result);
        }))
    }

    fn take_failure(&self) -> Result<()> {
        let failure = self
            .failure
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match failure {
            Some(failure) => bail!("cannot record the base evidence: {failure}"),
            None => Ok(()),
        }
    }
}

/// What the adapter recognising `command` reads from `result`, if one does.
pub(super) fn parsed(command: &str, cwd: &Path, result: &CriterionResult) -> Option<RunSummary> {
    let adapter = registry::recognize(command, cwd)?;
    Some(adapter.parse(&RunOutput {
        stdout: &result.stdout,
        stderr: &result.stderr,
        exit_code: result.exit_code,
    }))
}

/// The evidence of one run of criterion `index`, whose command the stage
/// defines as `command`, at the base commit `base`.
pub(super) fn evidence(
    index: usize,
    command: &str,
    base: &str,
    result: &CriterionResult,
    summary: Option<RunSummary>,
) -> BaseEvidence {
    let build_failed = summary
        .as_ref()
        .is_some_and(|summary| classify(summary, result.exit_code) == RunOutcome::BuildFailed);
    BaseEvidence {
        index,
        command: command.to_string(),
        base: base.to_string(),
        passed: result.success,
        executed: summary.as_ref().and_then(|summary| summary.executed),
        failed: summary.as_ref().and_then(|summary| summary.failed),
        build_failed,
        timed_out: result.timed_out,
    }
}

/// `at base <short>: <describe>`, then for a timed-out run the tests it left
/// running, bounded as an acceptance criterion's summary bounds them.
pub(super) fn base_summary(
    short: &str,
    evidence: &BaseEvidence,
    result: &CriterionResult,
) -> String {
    let mut summary = format!("at base {short}: {}", describe(evidence));
    if let Some(running) = result
        .running_at_timeout
        .as_ref()
        .filter(|_| result.timed_out)
    {
        summary.push('\n');
        summary.push_str(&running_block(running));
    }
    truncate_to_byte_limit(&summary, SUMMARY_MAX_BYTES)
}

/// `passed`, `failed <n> of <m>`, `selected zero tests`, `did not build` or
/// `timed out`.
pub(super) fn describe(evidence: &BaseEvidence) -> String {
    if evidence.timed_out {
        return "timed out".to_string();
    }
    if evidence.build_failed {
        return "did not build".to_string();
    }
    if evidence.executed == Some(0) {
        return "selected zero tests".to_string();
    }
    if evidence.passed {
        return "passed".to_string();
    }
    match (evidence.failed, evidence.executed) {
        (Some(failed), Some(executed)) => format!("failed {failed} of {executed}"),
        _ => "failed".to_string(),
    }
}
