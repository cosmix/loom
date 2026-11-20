//! What a session call prints (`wait` drives it): the `Host job: <state>
//! (<kind>, <short head>)` first line, a refusal, and a followed job's outcome.

use std::fmt::Write as _;
use std::io::Write;
use std::path::Path;

use anyhow::Result;

use super::digest;
use super::store::{job_dir, read_progress};
use super::wait::{WaitOutcome, WaitSite};
use super::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::verify::contracts::completion::guidance;

const SHORT_HEAD: usize = 12;

/// The line a passing trial ends with.
const TRIAL_PASSED: &str = "Trial passed: file the dispute with the same --replacement values.";

/// `Host job: <state> (<kind>, <short head>)`, the first line of every call.
pub fn heading(kind: JobKind, state: &str, head: &str) -> String {
    labelled(kind.name(), state, head)
}

/// [`heading`] for the job `site` follows: a contracts check and a trial
/// are named as such.
pub(super) fn site_heading(site: &WaitSite<'_>, state: &str, head: &str) -> String {
    let kind = match site.contracts {
        None => site.kind.name(),
        Some([]) => "contracts check",
        Some(_) => "contracts trial",
    };
    labelled(kind, state, head)
}

fn labelled(kind: &str, state: &str, head: &str) -> String {
    let short: String = head.chars().take(SHORT_HEAD).collect();
    format!("Host job: {state} ({kind}, {short})")
}

/// A refused call: its first line, then `message`. Returns the exit code, 1.
/// Every in-session error path of `loom check`, `loom stage complete` and the
/// contract freeze ends here, so the doctrine's "read the first line" holds
/// even when the call failed before it had a job.
pub fn refusal(out: &mut dyn Write, kind: JobKind, head: &str, message: &str) -> Result<i32> {
    refused(out, &heading(kind, "refused", head), message)
}

/// [`refusal`] under `site`'s heading.
pub(super) fn site_refusal(
    out: &mut dyn Write,
    site: &WaitSite<'_>,
    head: &str,
    message: &str,
) -> Result<i32> {
    refused(out, &site_heading(site, "refused", head), message)
}

fn refused(out: &mut dyn Write, first: &str, message: &str) -> Result<i32> {
    writeln!(out, "{first}")?;
    writeln!(out, "{message}")?;
    out.flush()?;
    Ok(1)
}

/// HEAD of the checkout at `dir`, or nothing when it cannot be read: a
/// refusal still prints its first line.
pub fn head_or_empty(dir: &Path) -> String {
    digest::head(dir).unwrap_or_default()
}

/// Print the outcome of a followed job: its first line, then the detail.
pub(super) fn report(
    site: &WaitSite<'_>,
    head: &str,
    outcome: &WaitOutcome,
    out: &mut dyn Write,
) -> Result<()> {
    let text = match outcome {
        WaitOutcome::Requested { .. } => return Ok(()),
        WaitOutcome::Refused(reason) => {
            site_refusal(out, site, head, reason)?;
            return Ok(());
        }
        WaitOutcome::Pending { done } => format!(
            "{}: {done} criteria done. Run the same command again to keep waiting.\n",
            site_heading(site, "pending", head)
        ),
        WaitOutcome::Finished(record, result) => {
            finished_text(site, head, record, result.as_ref())?
        }
    };
    out.write_all(text.as_bytes())?;
    out.flush()?;
    Ok(())
}

fn finished_text(
    site: &WaitSite<'_>,
    head: &str,
    record: &JobRecord,
    result: Option<&JobResult>,
) -> Result<String> {
    let word = match record.state {
        JobState::Passed => "passed",
        JobState::Failed => "failed",
        _ => "stopped",
    };
    let outcomes = match result {
        Some(result) => result.criteria.clone(),
        None => read_progress(site.work_dir, site.stage_id, &record.id)?,
    };
    let dir = job_dir(site.work_dir, site.stage_id, &record.id)?;
    let mut text = format!("{}\n", site_heading(site, word, head));
    for outcome in outcomes.iter().filter(|outcome| !outcome.passed) {
        write_failure(&mut text, &dir, outcome);
    }
    for outcome in &outcomes {
        let mark = if outcome.passed { '✓' } else { '✗' };
        let _ = writeln!(text, "{mark} {}", outcome.label);
    }
    if let Some(error) = result.and_then(|result| result.error.as_deref()) {
        let _ = writeln!(text, "{error}");
    }
    if let Some(detail) = &record.detail {
        let _ = writeln!(text, "{detail}");
    }
    if word == "stopped" {
        let _ = writeln!(text, "Run the same command again to request a new run.");
    }
    let contract_failed = |o: &CriterionOutcome| !o.passed && o.label.starts_with("contract:");
    if word == "failed" && outcomes.iter().any(contract_failed) {
        let _ = writeln!(text, "{}", guidance(site.stage_id));
    }
    if word == "passed" && site.contracts.is_some_and(|files| !files.is_empty()) {
        let _ = writeln!(text, "{TRIAL_PASSED}");
    }
    Ok(text)
}

fn write_failure(text: &mut String, dir: &Path, outcome: &CriterionOutcome) {
    let _ = writeln!(text, "{}", outcome.summary.trim_end());
    if let Some(log) = &outcome.log {
        let path = std::path::absolute(dir.join(log)).unwrap_or_else(|_| dir.join(log));
        let _ = writeln!(text, "full log: {}", path.display());
    }
}
