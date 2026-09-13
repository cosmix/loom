//! High-level acceptance criteria runner.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::cache::{self, CachePolicy};
use super::cache_contract::{AssertionVerdict, CriterionContract};
use super::cache_fingerprint::{self, ExecutionIdentity, InputFingerprint};
use super::config::CriteriaConfig;
use super::confine::{prepare_confined, resolve_confinement, CommandSpec, PreparedCommand};
use super::criterion_eval::check_criterion;
use super::executor::run_prepared_with_timeout;
use super::result::{AcceptanceResult, CriterionResult};
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
    let mut collected = CollectedResults::default();

    for criterion in &stage.acceptance {
        let prepared =
            prepare_criterion(criterion, setup.as_deref(), &context, config, confinement);
        let evaluated = run_criterion(prepared, working_dir, confinement, config)
            .with_context(|| format!("Failed to execute criterion: {}", criterion.command()))?;
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
    criterion: &'a AcceptanceCriterion,
    original_command: &'a str,
    spec: CommandSpec,
    timeout: Duration,
    contract: CriterionContract,
}

fn prepare_criterion<'a>(
    criterion: &'a AcceptanceCriterion,
    setup: Option<&str>,
    context: &CriteriaContext,
    config: &CriteriaConfig,
    confinement: CommandConfinement,
) -> PreparedCriterion<'a> {
    let original_command = criterion.command();
    let expanded = context.expand(original_command);
    let full_command = setup
        .map(|prefix| format!("{prefix} && {expanded}"))
        .unwrap_or(expanded);
    let timeout = if criterion.is_extended() {
        Duration::from_secs(30)
    } else {
        config.command_timeout
    };
    let spec = CommandSpec::shell(full_command);
    let contract = CriterionContract::new(&spec, criterion, timeout, confinement);
    PreparedCriterion {
        criterion,
        original_command,
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
    let mut outcome = run_with_cache(
        &prepared.contract,
        &prepared.spec,
        working_dir,
        prepared.timeout,
        confinement,
        config,
    )?;
    outcome.result.command = prepared.original_command.to_string();
    if outcome.result.cached {
        return Ok(EvaluatedCriterion {
            result: outcome.result,
            failures: Vec::new(),
        });
    }
    let verdict = prepared.contract.verdict(&outcome.result);
    let failures = check_criterion(
        prepared.criterion,
        &prepared.contract,
        &outcome.result,
        prepared.original_command,
        &verdict,
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
    contract: &CriterionContract,
    spec: &CommandSpec,
    working_dir: Option<&Path>,
    timeout: Duration,
    confinement: CommandConfinement,
    config: &CriteriaConfig,
) -> Result<RunOutcome> {
    let started = Instant::now();
    let prepared = prepare_confined(spec, working_dir, confinement)?;
    let Some(work_dir) = enabled_cache_dir(config) else {
        return execute(prepared, timeout, started, None);
    };
    let acceptance_dir = working_dir.unwrap_or(Path::new("."));
    if !cache::is_cacheable(&spec.to_string(), acceptance_dir) {
        return execute(prepared, timeout, started, None);
    }
    let Some(identity) = ExecutionIdentity::from_prepared(&prepared, confinement) else {
        return execute(prepared, timeout, started, None);
    };
    let Some(before) = cache_fingerprint::capture(acceptance_dir, &identity) else {
        return execute(prepared, timeout, started, None);
    };
    if let Some(record) = cache::lookup_pass(work_dir, contract, &before) {
        let result = CriterionResult::cached_verdict(
            spec.to_string(),
            Some(record.actual_exit),
            record.stdout_tail,
            record.stderr_tail,
            started.elapsed(),
        );
        return Ok(RunOutcome {
            result,
            execution_duration: None,
            candidate: None,
        });
    }
    let candidate = CacheCandidate {
        work_dir: work_dir.to_path_buf(),
        acceptance_dir: acceptance_dir.to_path_buf(),
        before,
        identity,
    };
    execute(prepared, timeout, started, Some(candidate))
}

fn enabled_cache_dir(config: &CriteriaConfig) -> Option<&Path> {
    config
        .cache_dir
        .as_deref()
        .filter(|_| config.cache == CachePolicy::Use)
}

fn execute(
    prepared: PreparedCommand,
    timeout: Duration,
    started: Instant,
    candidate: Option<CacheCandidate>,
) -> Result<RunOutcome> {
    let mut result = run_prepared_with_timeout(prepared, timeout)?;
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

#[derive(Default)]
struct CollectedResults {
    results: Vec<CriterionResult>,
    failures: Vec<String>,
}

struct EvaluatedCriterion {
    result: CriterionResult,
    failures: Vec<String>,
}

impl CollectedResults {
    fn push(&mut self, evaluated: EvaluatedCriterion) {
        self.results.push(evaluated.result);
        self.failures.extend(evaluated.failures);
    }

    fn warn_on_suspicious_stderr(&self) {
        for result in &self.results {
            for warning in detect_stderr_warnings(result) {
                eprintln!("warning: {warning}");
            }
        }
    }

    fn finish(self) -> AcceptanceResult {
        if self.failures.is_empty() {
            AcceptanceResult::AllPassed {
                results: self.results,
            }
        } else {
            AcceptanceResult::Failed {
                results: self.results,
                failures: self.failures,
            }
        }
    }
}

fn detect_stderr_warnings(result: &CriterionResult) -> Vec<String> {
    if !result.success || result.stderr.is_empty() || result.cached {
        return Vec::new();
    }
    let patterns = [
        "connection refused",
        "permission denied",
        "failed to download",
        "blocked",
        "EACCES",
        "ECONNREFUSED",
        "unable to connect",
        "network error",
        "sandbox",
    ];
    let stderr = result.stderr.to_lowercase();
    patterns
        .iter()
        .filter(|pattern| stderr.contains(pattern.to_lowercase().as_str()))
        .map(|pattern| {
            format!(
                "Command '{}' succeeded (exit 0) but stderr contains '{}' — may indicate a silent failure",
                result.command, pattern
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(success: bool, stderr: &str) -> CriterionResult {
        CriterionResult::new(
            "test-command".to_string(),
            success,
            String::new(),
            stderr.to_string(),
            if success { Some(0) } else { Some(1) },
            Duration::from_millis(100),
            false,
        )
    }

    #[test]
    fn stderr_warning_detection_is_case_insensitive_and_success_only() {
        let warning = make_result(true, "BLOCKED by firewall, connection refused");
        let failure = make_result(false, "connection refused");
        assert_eq!(detect_stderr_warnings(&warning).len(), 2);
        assert!(detect_stderr_warnings(&failure).is_empty());
    }

    #[test]
    fn ordinary_success_stderr_is_not_a_warning() {
        let result = make_result(true, "Compiling project\nFinished dev target");
        assert!(detect_stderr_warnings(&result).is_empty());
    }
}
