//! High-level acceptance criteria runner.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::cache;
use super::cache_contract::{AssertionVerdict, CachedCriterionPass, CriterionContract};
use super::cache_fingerprint::{self, ExecutionIdentity, InputFingerprint};
use super::config::{extended_criterion_timeout, CriteriaConfig};
use super::confine::{prepare_confined, resolve_confinement, CommandSpec, PreparedCommand};
use super::criterion_eval::{evaluate, CriterionCommand};
use super::executor::run_prepared_live;
use super::live_output::LiveSink;
use super::result::{AcceptanceResult, CriterionResult};
use super::runner_results::{note_running_tests, CollectedResults, EvaluatedCriterion};
use crate::models::stage::{AcceptanceCriterion, CommandConfinement, Stage};
use crate::verify::context::CriteriaContext;

pub fn run_acceptance(stage: &Stage, working_dir: Option<&Path>) -> Result<AcceptanceResult> {
    run_acceptance_with_config(stage, working_dir, &CriteriaConfig::default())
}

/// Run every criterion against its complete semantic and execution contract.
pub fn run_acceptance_with_config(
    stage: &Stage,
    working_dir: Option<&Path>,
    config: &CriteriaConfig,
) -> Result<AcceptanceResult> {
    config.check_selection(stage)?;
    if stage.acceptance.is_empty() {
        return Ok(AcceptanceResult::AllPassed {
            results: Vec::new(),
        });
    }
    let confinement =
        resolve_confinement(stage.sandbox.command_confinement, config.plan_confinement);
    let default_dir = PathBuf::from(".");
    let context = CriteriaContext::with_stage_id(working_dir.unwrap_or(&default_dir), &stage.id);
    let setup = expanded_setup(stage, &context);
    let zero_test_guard = stage.plan_version == 2;
    let mut collected = CollectedResults::default();

    let selected = stage.acceptance.iter().enumerate();
    for (index, criterion) in selected.filter(|(index, _)| config.selects(*index)) {
        let prepared = prepare_criterion(
            index,
            criterion,
            setup.as_deref(),
            &context,
            config,
            confinement,
            zero_test_guard,
        );
        let evaluated = run_criterion(prepared, working_dir, confinement, config)
            .with_context(|| format!("Failed to execute criterion: {}", criterion.command()))?;
        if let Some(observer) = &config.observer {
            observer.notify(index, &evaluated.result);
        }
        collected.push(evaluated);
    }
    collected.warn_on_suspicious_stderr();
    Ok(collected.finish())
}

fn expanded_setup(stage: &Stage, context: &CriteriaContext) -> Option<String> {
    (!stage.setup.is_empty()).then(|| {
        stage
            .setup
            .iter()
            .map(|command| context.expand(command))
            .collect::<Vec<_>>()
            .join(" && ")
    })
}

struct PreparedCriterion<'a> {
    index: usize,
    criterion: &'a AcceptanceCriterion,
    original_command: &'a str,
    /// The criterion's command with variables expanded and no setup prefix.
    expanded_command: String,
    spec: CommandSpec,
    timeout: Duration,
    contract: CriterionContract,
}

fn prepare_criterion<'a>(
    index: usize,
    criterion: &'a AcceptanceCriterion,
    setup: Option<&str>,
    context: &CriteriaContext,
    config: &CriteriaConfig,
    confinement: CommandConfinement,
    zero_test_guard: bool,
) -> PreparedCriterion<'a> {
    let original_command = criterion.command();
    let expanded = context.expand(original_command);
    let full_command = setup
        .map(|prefix| format!("{prefix} && {expanded}"))
        .unwrap_or_else(|| expanded.clone());
    let timeout = match criterion {
        AcceptanceCriterion::Simple(_) => config.command_timeout,
        AcceptanceCriterion::Extended(check) => extended_criterion_timeout(check),
    };
    let spec = CommandSpec::shell(full_command);
    let contract = CriterionContract::new(&spec, criterion, timeout, confinement)
        .with_zero_test_guard(zero_test_guard);
    PreparedCriterion {
        index,
        criterion,
        original_command,
        expanded_command: expanded,
        spec,
        timeout,
        contract,
    }
}

fn run_criterion(
    prepared: PreparedCriterion<'_>,
    working_dir: Option<&Path>,
    confinement: CommandConfinement,
    config: &CriteriaConfig,
) -> Result<EvaluatedCriterion> {
    let mut outcome = run_with_cache(&prepared, working_dir, confinement, config)?;
    outcome.result.command = prepared.original_command.to_string();
    if outcome.result.cached {
        return Ok(EvaluatedCriterion {
            result: outcome.result,
            failures: Vec::new(),
        });
    }
    let cwd = working_dir.unwrap_or(Path::new("."));
    note_running_tests(&mut outcome.result, &prepared.expanded_command, cwd);
    let command = CriterionCommand {
        written: prepared.original_command,
        expanded: &prepared.expanded_command,
        cwd,
    };
    let (verdict, failures) = evaluate(
        prepared.criterion,
        &prepared.contract,
        &outcome.result,
        &command,
    );
    outcome.result.success = verdict.passed;
    if outcome.result.success {
        publish_pass(
            &prepared.contract,
            &outcome.result,
            outcome.execution_duration,
            verdict,
            outcome.candidate,
        );
    }
    Ok(EvaluatedCriterion {
        result: outcome.result,
        failures,
    })
}

struct RunOutcome {
    result: CriterionResult,
    execution_duration: Option<Duration>,
    candidate: Option<CacheCandidate>,
}

struct CacheCandidate {
    work_dir: PathBuf,
    acceptance_dir: PathBuf,
    before: InputFingerprint,
    identity: ExecutionIdentity,
}

fn run_with_cache(
    criterion: &PreparedCriterion<'_>,
    working_dir: Option<&Path>,
    confinement: CommandConfinement,
    config: &CriteriaConfig,
) -> Result<RunOutcome> {
    let (contract, spec) = (&criterion.contract, &criterion.spec);
    let started = Instant::now();
    let run = |prepared, candidate| {
        let live = config
            .live_output
            .as_ref()
            .and_then(|live| live.open(criterion.index));
        execute(prepared, criterion.timeout, started, candidate, live)
    };
    let tree = config.tree_root.as_deref();
    let cache = config.stage_cache.as_ref();
    let prepared = prepare_confined(spec, working_dir, tree, confinement, cache)?;
    let Some(work_dir) = enabled_cache_dir(config) else {
        return run(prepared, None);
    };
    let acceptance_dir = working_dir.unwrap_or(Path::new("."));
    if !cache::is_cacheable(&spec.to_string(), acceptance_dir) {
        return run(prepared, None);
    }
    let Some(identity) = ExecutionIdentity::from_prepared(&prepared, confinement) else {
        return run(prepared, None);
    };
    let Some(before) = cache_fingerprint::capture(acceptance_dir, &identity) else {
        return run(prepared, None);
    };
    let lookup = || cache::lookup_pass(work_dir, contract, &before);
    if let Some(record) = config.cache.reads().then(lookup).flatten() {
        return Ok(cached_outcome(spec, record, started));
    }
    let candidate = CacheCandidate {
        work_dir: work_dir.to_path_buf(),
        acceptance_dir: acceptance_dir.to_path_buf(),
        before,
        identity,
    };
    run(prepared, Some(candidate))
}

/// The outcome of a criterion answered by the pass cache: nothing ran.
fn cached_outcome(spec: &CommandSpec, record: CachedCriterionPass, started: Instant) -> RunOutcome {
    let result = CriterionResult::cached_verdict(
        spec.to_string(),
        Some(record.actual_exit),
        record.stdout_tail,
        record.stderr_tail,
        record.verdict.tests_executed,
        started.elapsed(),
    );
    RunOutcome {
        result,
        execution_duration: None,
        candidate: None,
    }
}

fn enabled_cache_dir(config: &CriteriaConfig) -> Option<&Path> {
    let dir = config.cache_dir.as_deref()?;
    config.cache.writes().then_some(dir)
}

fn execute(
    prepared: PreparedCommand,
    timeout: Duration,
    started: Instant,
    candidate: Option<CacheCandidate>,
    live: Option<LiveSink>,
) -> Result<RunOutcome> {
    let mut result = run_prepared_live(prepared, timeout, live)?;
    let execution_duration = result.duration;
    result.duration = started.elapsed();
    Ok(RunOutcome {
        result,
        execution_duration: Some(execution_duration),
        candidate,
    })
}

fn publish_pass(
    contract: &CriterionContract,
    result: &CriterionResult,
    execution_duration: Option<Duration>,
    verdict: AssertionVerdict,
    candidate: Option<CacheCandidate>,
) {
    let (Some(candidate), Some(execution_duration)) = (candidate, execution_duration) else {
        return;
    };
    let Some(after) = cache_fingerprint::capture(&candidate.acceptance_dir, &candidate.identity)
    else {
        return;
    };
    if after != candidate.before {
        return;
    }
    let _ = cache::store_pass(
        &candidate.work_dir,
        contract,
        &after,
        result,
        execution_duration,
        verdict,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::criteria::CriterionObserver;
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    #[test]
    fn only_runs_the_named_criteria_under_their_own_indices() {
        let dir = tempfile::TempDir::new().unwrap();
        let mut stage = Stage::new("subset".to_string(), None);
        stage.sandbox.command_confinement = Some(CommandConfinement::Inherit); // selection only
        for index in 0..3 {
            let command = format!("touch ran-{index}");
            stage.add_acceptance_criterion(AcceptanceCriterion::Simple(command));
        }
        let seen = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&seen);
        let observer = CriterionObserver(Arc::new(move |index: usize, _: &CriterionResult| {
            observed.lock().unwrap().push(index);
        }));
        let past_end = CriteriaConfig::default().with_only(Some(BTreeSet::from([3])));
        assert!(run_acceptance_with_config(&stage, Some(dir.path()), &past_end).is_err());
        let config = CriteriaConfig::default().with_only(Some(BTreeSet::from([0, 2])));
        let config = config.with_observer(observer);

        let result = run_acceptance_with_config(&stage, Some(dir.path()), &config).unwrap();

        assert!(result.all_passed());
        assert_eq!(result.results().len(), 2);
        assert_eq!(result.results()[1].command, "touch ran-2");
        assert_eq!(*seen.lock().unwrap(), [0, 2]);
        assert!(!dir.path().join("ran-1").exists(), "criterion 1 ran");
    }
}

#[cfg(test)]
#[path = "runner_timeout_tests.rs"]
mod timeout_tests;
