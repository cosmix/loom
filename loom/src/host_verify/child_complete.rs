//! The host child's completion run (`JobKind::Complete`): what
//! `loom stage complete` verifies, run by the daemon on the host for the
//! committed content the request named.
//!
//! A knowledge stage runs the snapshot and its acceptance in the repository
//! root. Any other stage captures the snapshot (the tree committed and clean,
//! HEAD at the stage branch tip, which must be the request's head), runs
//! acceptance, then every completion check (`complete_verification`) with the
//! daemon's gates and fail-closed detection: test integrity and the review
//! gate are checked when the daemon applies the result. The checks run only
//! once acceptance passes, as on the operator's path; each one then runs even
//! after an earlier one failed, so one run names every failing check. The
//! result carries the snapshot's policy hash, which the application binds.

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, ensure, Context, Result};

use super::child::{finish, run_acceptance, JobTree, Progress, SUMMARY_MAX_BYTES};
use super::{CriterionOutcome, JobRecord, JobResult};
use crate::commands::stage::complete::complete_impact::observed_impact;
use crate::commands::stage::complete::complete_verification::{
    run_checks, VerificationChecks, IMPACT_TESTS_LABEL,
};
use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::{Stage, StageType};
use crate::verify::impact_tests::{record_observed, ObservedImpact};
use crate::verify::transitions::load_stage;

/// What the run learned besides its outcomes.
#[derive(Default)]
struct Learned {
    /// The snapshot's effective policy hash, once captured.
    policy: Option<String>,
    /// Whether the stage keeps an impact-test record (a plan v2 standard
    /// stage) and what the latest impact selection observed.
    impact: Option<ObservedImpact>,
}

/// Run the completion job `record` and return its result. Every failed run
/// of a plan v2 standard stage replaces the stage's observed impact-test
/// record, so a dispute names only failures of the latest run.
pub(super) fn run(work_dir: &Path, record: &JobRecord) -> JobResult {
    let progress = Arc::new(Progress::new(work_dir, &record.stage_id, &record.id));
    let mut learned = Learned::default();
    let checked = verify(work_dir, record, &progress, &mut learned);
    let result = finish(record, progress.take_outcomes(), checked, learned.policy);
    record_failed_impact(work_dir, &record.stage_id, learned.impact, result.passed);
    result
}

/// Replace the stage's observed impact-test record with `impact` when the
/// run failed; a passing run, or a stage that keeps no record (`None`),
/// writes nothing. A record that cannot be kept is a warning; the run's own
/// result stands.
pub(super) fn record_failed_impact(
    work_dir: &Path,
    stage_id: &str,
    impact: Option<ObservedImpact>,
    passed: bool,
) {
    let Some(observed) = impact.filter(|_| !passed) else {
        return;
    };
    if let Err(error) = record_observed(work_dir, stage_id, &observed) {
        eprintln!("Warning: the impact-test failures were not recorded: {error:#}");
    }
}

/// Learn what the impact check's result observed: the failures of its error,
/// or none when it passed or failed for another reason. Any other check, and
/// a stage that keeps no record, leave `impact` as it is.
pub(super) fn observe_impact(
    impact: &mut Option<ObservedImpact>,
    label: &str,
    checked: &Result<()>,
) {
    if let Some(impact) = impact.as_mut().filter(|_| label == IMPACT_TESTS_LABEL) {
        *impact = checked
            .as_ref()
            .err()
            .map(observed_impact)
            .unwrap_or_default();
    }
}

fn verify(
    work_dir: &Path,
    record: &JobRecord,
    progress: &Arc<Progress>,
    learned: &mut Learned,
) -> Result<Option<String>> {
    let stage = load_stage(&record.stage_id, work_dir)
        .with_context(|| format!("Failed to load stage '{}'", record.stage_id))?;
    if stage.plan_version == 2 && stage.stage_type == StageType::Standard {
        learned.impact = Some(ObservedImpact::default());
    }
    let tree = JobTree::resolve(&stage, work_dir)?;
    if let Some(mismatch) = tree.differences(record)? {
        bail!("the tree does not match the request: {mismatch}");
    }
    let snapshot = VerificationSnapshot::capture(&stage, work_dir)?;
    ensure!(
        snapshot.commit == record.head,
        "the stage's committed content is {}, the request named {}",
        snapshot.commit,
        record.head
    );
    learned.policy = Some(snapshot.policy.clone());
    run_acceptance(&stage, &tree, work_dir, record.no_cache, progress)?;
    progress.take_failure()?;
    if !tree.knowledge && !progress.any_failed() {
        run_completion_checks(&stage, &tree, work_dir, progress, learned)?;
        progress.take_failure()?;
    }
    let changed = tree.differences(record)?;
    if changed.is_none() {
        snapshot.validate(&stage, work_dir)?;
    }
    Ok(changed)
}

/// Every completion check, each recorded as an outcome under its label.
fn run_completion_checks(
    stage: &Stage,
    tree: &JobTree,
    work_dir: &Path,
    progress: &Progress,
    learned: &mut Learned,
) -> Result<()> {
    let checks = VerificationChecks {
        stage,
        stage_id: &stage.id,
        acceptance_dir: tree.paths.acceptance_dir.as_deref(),
        worktree_root: tree.paths.worktree_root.as_deref(),
        daemon_gates: true,
        fail_closed: true,
        work_dir,
    };
    run_checks(&checks, &mut |label, checked| {
        observe_impact(&mut learned.impact, label, &checked);
        progress.record(check_outcome(label, &checked), None);
        Ok(())
    })
}

/// A check's outcome: a failure carries its message as the summary.
fn check_outcome(label: &str, checked: &Result<()>) -> CriterionOutcome {
    let summary = match checked {
        Ok(()) => format!("✓ {label}"),
        Err(error) => format!("✗ {label}: {error:#}"),
    };
    CriterionOutcome {
        label: label.to_string(),
        passed: checked.is_ok(),
        summary: truncate_to_byte_limit(&summary, SUMMARY_MAX_BYTES),
        log: None,
    }
}
