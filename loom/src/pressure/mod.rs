//! Shared pressure pipeline for terminal and planning-session drivers.
//!
//! Drivers run Claude `/pressure`, Codex `$pressure`, and Claude `/address`;
//! this module orders rounds, records their results, and coordinates cleanup.

use std::path::Path;

use anyhow::Result;

pub mod codex_run;
pub mod instruction;
pub mod models;
pub mod paths;
pub mod record;
pub mod steps;

pub use instruction::{completion_instruction, MarkerHome};
pub use steps::{plan_steps, Step, RECORD_STEP_LABEL};

/// Whether the next pipeline step may run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepVerdict {
    Continue,
    Stop { step: String, reason: String },
}

/// The outcome of concurrent pressure runs, including the actual Codex model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PressureResult {
    pub verdict: StepVerdict,
    pub codex_model_used: Option<String>,
}

/// The execution and cleanup operations supplied by each user interface.
pub trait PressureDriver {
    /// Delete `report` if present (each driver may log it).
    fn delete_report(&mut self, report: &Path) -> Result<()>;
    /// Run Claude `/pressure` and Codex `$pressure` concurrently; return when both ended.
    fn run_pressure(&mut self, round: u32, claude: &str, codex: &str) -> Result<PressureResult>;
    /// Run Claude `/address`.
    fn run_address(&mut self, round: u32, slash: &str) -> Result<StepVerdict>;
}

/// The plan, models, and date fixed at the start of a pipeline run.
pub struct PipelineInput<'a> {
    pub rounds: u32,
    pub invocation: &'a str,
    pub report: &'a Path,
    /// Absolute filesystem path at which the run started.
    pub plan_path: &'a Path,
    /// The model assigned to the `/pressure` slot.
    pub claude_model: &'a str,
    pub today: chrono::NaiveDate,
}

/// The completed record or the first step that stopped the pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineOutcome {
    Recorded { line: String },
    RecordSkipped { line: String, why: String },
    Stopped { step: String, reason: String },
}

#[derive(Default)]
struct RoundResults {
    round: u32,
    codex_models: Vec<String>,
    missing_rounds: Vec<u32>,
}

/// Run the shared step sequence, writing its record before final cleanup.
pub fn run_pipeline(
    driver: &mut dyn PressureDriver,
    input: &PipelineInput<'_>,
) -> Result<PipelineOutcome> {
    anyhow::ensure!(
        input.rounds >= 1,
        "pressure pipeline needs at least one round"
    );
    let mut results = RoundResults::default();
    let mut record_result = None;
    for step in plan_steps(input.rounds, input.invocation, input.report) {
        let verdict = match step {
            Step::Record => {
                record_result = Some(write_record(input, std::mem::take(&mut results)));
                continue;
            }
            Step::DeleteReport(report) if record_result.is_some() => {
                let deleted = driver.delete_report(&report);
                return match record_result.expect("record result was checked") {
                    Ok(outcome) => {
                        deleted?;
                        Ok(outcome)
                    }
                    Err(error) => {
                        if let Err(cleanup) = deleted {
                            tracing::warn!(error = %cleanup, "cannot delete the Codex report");
                        }
                        Err(error)
                    }
                };
            }
            step => match run_step(driver, input.report, &mut results, step) {
                Ok(verdict) => verdict,
                Err(error) => {
                    // The report must not outlive a failed step either; the step's error wins.
                    if let Err(cleanup) = driver.delete_report(input.report) {
                        tracing::warn!(error = %cleanup, "cannot delete the Codex report");
                    }
                    return Err(error);
                }
            },
        };
        if let StepVerdict::Stop { step, reason } = verdict {
            driver.delete_report(input.report)?;
            return Ok(PipelineOutcome::Stopped { step, reason });
        }
    }
    anyhow::bail!("pressure pipeline did not contain final report cleanup")
}

fn run_step(
    driver: &mut dyn PressureDriver,
    report: &Path,
    results: &mut RoundResults,
    step: Step,
) -> Result<StepVerdict> {
    match step {
        Step::DeleteReport(path) => {
            driver.delete_report(&path)?;
            Ok(StepVerdict::Continue)
        }
        Step::Pressure { claude, codex } => {
            results.round += 1;
            let result = driver.run_pressure(results.round, &claude, &codex)?;
            results
                .codex_models
                .push(result.codex_model_used.unwrap_or_default());
            Ok(result.verdict)
        }
        Step::Address(slash) => {
            if !report.is_file() {
                results.missing_rounds.push(results.round);
            }
            driver.run_address(results.round, &slash)
        }
        Step::Record => unreachable!("record is handled by run_pipeline"),
    }
}

fn write_record(input: &PipelineInput<'_>, results: RoundResults) -> Result<PipelineOutcome> {
    let line = record::RecordLine {
        date: input.today,
        claude_model: input.claude_model.to_string(),
        codex_models: results.codex_models,
        rounds: input.rounds,
        missing_rounds: results.missing_rounds,
    };
    let rendered = line.render();
    let outcome = match record::append_record(input.plan_path, &line)? {
        record::RecordWrite::Written => PipelineOutcome::Recorded { line: rendered },
        record::RecordWrite::PlanMoved => PipelineOutcome::RecordSkipped {
            line: rendered,
            why: "plan moved or changed while saving the record".to_string(),
        },
    };
    Ok(outcome)
}

#[cfg(test)]
mod tests;
