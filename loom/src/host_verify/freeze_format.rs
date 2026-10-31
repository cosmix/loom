//! Outcome formatting shared by the contract run and its format gate.

use anyhow::Result;

use super::{child, CriterionOutcome};
use crate::daemon::ContractRunReport;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::CommandConfinement;
use crate::verify::contracts::freeze::{check_formatting, refused_problems};
use crate::verify::contracts::site::ContractSite;

/// The summary of a contract the run classified red; `shared_from` names the
/// contract whose failed build it took instead of running its own.
pub(super) fn verdict_line(report: &ContractRunReport, shared_from: Option<&str>) -> String {
    let id = &report.contract_id;
    let exit = report
        .exit_code
        .map_or_else(|| "no exit code".to_string(), |code| format!("exit {code}"));
    let shared = shared_from.map_or_else(String::new, |first| {
        format!("; shares the failed build of contract `{first}`, not run again")
    });
    format!(
        "contract `{id}` is red ({}, {exit}), as the freeze requires{shared}",
        report.outcome
    )
}

/// The format gate as outcome `format`. A gate that could not run is an
/// error of the run, never a failed outcome.
pub(super) fn format_outcome(
    site: &ContractSite,
    confinement: CommandConfinement,
) -> Result<CriterionOutcome> {
    let (passed, summary) = match check_formatting(site, confinement) {
        Ok(()) => (
            true,
            "every contract and harness file passes the stage's formatter checks".to_string(),
        ),
        Err(error) => match refused_problems(&error) {
            Some(problems) => (false, problems.join("\n")),
            None => return Err(error),
        },
    };
    Ok(outcome("format".to_string(), passed, &summary))
}

pub(super) fn outcome(label: String, passed: bool, summary: &str) -> CriterionOutcome {
    CriterionOutcome {
        label,
        passed,
        summary: truncate_to_byte_limit(summary, child::SUMMARY_MAX_BYTES),
        log: None,
    }
}
