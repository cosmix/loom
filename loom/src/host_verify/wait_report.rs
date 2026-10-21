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

const SHORT_HEAD: usize = 12;

/// `Host job: <state> (<kind>, <short head>)`, the first line of every call.
pub fn heading(kind: JobKind, state: &str, head: &str) -> String {
    let short: String = head.chars().take(SHORT_HEAD).collect();
    format!("Host job: {state} ({}, {short})", kind.name())
}

/// A refused call: its first line, then `message`. Returns the exit code, 1.
/// Every in-session error path of `loom check`, `loom stage complete` and the
/// contract freeze ends here, so the doctrine's "read the first line" holds
/// even when the call failed before it had a job.
pub fn refusal(out: &mut dyn Write, kind: JobKind, head: &str, message: &str) -> Result<i32> {
    writeln!(out, "{}", heading(kind, "refused", head))?;
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
        WaitOutcome::Requested => return Ok(()),
        WaitOutcome::Refused(reason) => {
            refusal(out, site.kind, head, reason)?;
            return Ok(());
        }
        WaitOutcome::Pending { done } => format!(
            "{}: {done} criteria done. Run the same command again to keep waiting.\n",
            heading(site.kind, "pending", head)
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
    let mut text = format!("{}\n", heading(site.kind, word, head));
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
    Ok(text)
}

fn write_failure(text: &mut String, dir: &Path, outcome: &CriterionOutcome) {
    let _ = writeln!(text, "{}", outcome.summary.trim_end());
    if let Some(log) = &outcome.log {
        let path = std::path::absolute(dir.join(log)).unwrap_or_else(|_| dir.join(log));
        let _ = writeln!(text, "full log: {}", path.display());
    }
}
