//! The impact-test record of a failed completion (DESIGN D14, D15): how the
//! failures a completion reports reach the stage's observed record
//! (`verify::impact_tests::observed`), which a dispute of them names.
//!
//! A stage session's `loom stage complete` runs in its sandbox and writes no
//! loom state, so on failure it prints the record as one
//! `LOOM_CONTROL_IMPACT_V1 <json>` line and a `LOOM_CONTROL_IMPACT_EOF` line,
//! the last two lines of its output. The completion broker, run by the
//! PostToolUse hook outside the sandbox on the output of exactly the pinned
//! `loom stage complete <stage>`, writes it. The record is not authenticated:
//! that run executes the stage's own code, which can print a record of its
//! choosing and end loom before loom prints its own. What the broker keeps is
//! the stage session's report. The adjudicator, who re-runs each disputed
//! selection, is the trust gate, and an accepted failure excuses only a later
//! failure identical to it (`verify::impact_tests::accepted`). Every failing
//! completion the broker sees replaces the record, so a dispute names only
//! failures of the latest run. A completion outside a stage session has no
//! sandbox to cross and writes the record itself.

use anyhow::{bail, Context, Result};
use std::path::Path;

use crate::models::stage::{Stage, StageType};
use crate::verify::impact_tests::{record_observed, ImpactTestsFailed, ObservedImpact};

pub const IMPACT_RECORD_PREFIX: &str = "LOOM_CONTROL_IMPACT_V1 ";
pub const IMPACT_EOF_MARKER: &str = "LOOM_CONTROL_IMPACT_EOF";

/// After a completion of `stage_id` failed with `error`: print the impact
/// record for the broker when `stage_session` (the run is sandboxed), else
/// record the failures the impact gate observed straight away. A record that
/// cannot be kept is a warning; the completion's own error stands.
pub fn after_failed_completion(
    stage_id: &str,
    stage_session: bool,
    work_dir: &Path,
    error: &anyhow::Error,
) {
    let failures = error
        .downcast_ref::<ImpactTestsFailed>()
        .map(|failed| failed.failures.as_slice())
        .unwrap_or_default();
    let observed = ObservedImpact::of(failures);
    if stage_session {
        match format_impact_record(&observed) {
            Ok(record) => print!("{record}"),
            Err(error) => eprintln!("Warning: the impact-test record was not printed: {error:#}"),
        }
    } else if let Err(error) = record_observed(work_dir, stage_id, &observed) {
        eprintln!("Warning: the impact-test failures were not recorded: {error:#}");
    }
}

/// The record lines for `observed`.
pub fn format_impact_record(observed: &ObservedImpact) -> Result<String> {
    let json = serde_json::to_string(observed).context("serializing the impact-test record")?;
    Ok(format!(
        "{IMPACT_RECORD_PREFIX}{json}\n{IMPACT_EOF_MARKER}\n"
    ))
}

/// The record a failed completion printed in `output`: `None` when it printed
/// none. The record line and its marker must be the last two non-empty lines
/// of `output` and appear nowhere else in it; fails otherwise, and on a
/// record loom would not write.
pub fn parse_impact_record(output: &str) -> Result<Option<ObservedImpact>> {
    let lines: Vec<&str> = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let parts = lines
        .iter()
        .filter(|line| line.starts_with(IMPACT_RECORD_PREFIX) || **line == IMPACT_EOF_MARKER)
        .count();
    if parts == 0 {
        return Ok(None);
    }
    let json = match lines.as_slice() {
        [.., record, marker] if parts == 2 && *marker == IMPACT_EOF_MARKER => {
            record.strip_prefix(IMPACT_RECORD_PREFIX)
        }
        _ => None,
    };
    let Some(json) = json else {
        bail!("the output does not end with one impact-test record followed by its marker");
    };
    let observed: ObservedImpact =
        serde_json::from_str(json).context("invalid impact-test record")?;
    observed.check()?;
    Ok(Some(observed))
}

/// Replace `stage`'s observed record with the impact-test record a failed
/// stage-session completion printed in `output`. Output holding no readable
/// record clears it, as a record of no failure does: a dispute names only
/// failures of the latest completion run. Only a plan v2 standard stage runs
/// the impact gate; any other stage's output is left alone.
pub fn record_from_output(stage: &Stage, work_dir: &Path, output: &str) -> Result<()> {
    if stage.plan_version != 2 || stage.stage_type != StageType::Standard {
        return Ok(());
    }
    let parsed = parse_impact_record(output);
    let observed = parsed.as_ref().ok().cloned().flatten().unwrap_or_default();
    record_observed(work_dir, &stage.id, &observed)?;
    parsed.map(|_| ())
}

#[cfg(test)]
#[path = "impact_record_tests.rs"]
mod tests;
