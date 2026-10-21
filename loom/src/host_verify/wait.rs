//! The session side of a host job: request one, then follow it across Bash
//! calls. A call that finds no matching request state relays a ticket and
//! returns at once; a call that finds one waits for the job inside its budget.
//! Both exit 0 while nothing is decided, because Claude Code runs the relay
//! and heartbeat hooks only after a Bash call that exits 0.

use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::digest::{head, knowledge_tree_digest, tree_digest};
use super::store::{job_dir, load_current, load_job_record, load_result, read_progress};
use super::{CriterionOutcome, JobKind, JobRecord, JobResult, JobState};
use crate::fs::inbox::{read_ledger, LedgerOutcome};
use crate::models::stage::Stage;
use crate::relay::HostJobRequest;

/// How long a request may sit with no ledger outcome and no job naming it
/// before the call reports that it never reached the daemon.
const REQUEST_GRACE: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_secs(1);
const SHORT_HEAD: usize = 12;

/// Where a wait runs and what it waits for.
pub struct WaitSite<'a> {
    pub work_dir: &'a Path,
    pub stage_id: &'a str,
    pub session_id: &'a str,
    pub checkout: &'a Path,
    pub knowledge: bool,
    pub kind: JobKind,
    pub no_cache: bool,
    pub scratch_dir: &'a Path,
    pub budget: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitOutcome {
    Requested,
    Pending { done: usize },
    Finished(Box<JobRecord>, Option<JobResult>),
    Refused(String),
}

/// The request this session last relayed for a kind of job.
#[derive(Debug, Serialize, Deserialize)]
struct RequestState {
    request_id: String,
    head: String,
    tree_digest: String,
    no_cache: bool,
    submitted_at: DateTime<Utc>,
}

/// The process exit code of an outcome: 0 while undecided or passed, 1 for a
/// failure or refusal, 4 for a stopped job.
pub fn exit_code(outcome: &WaitOutcome) -> i32 {
    match outcome {
        WaitOutcome::Requested | WaitOutcome::Pending { .. } => 0,
        WaitOutcome::Refused(_) => 1,
        WaitOutcome::Finished(record, _) => match record.state {
            JobState::Failed => 1,
            JobState::Cancelled | JobState::Interrupted => 4,
            JobState::Queued | JobState::Running | JobState::Passed => 0,
        },
    }
}

/// Seconds a call may wait: `min(100, 3 * subagent timeout - 60)`, at least
/// 30, with a disabled timeout counting as 100, so the call returns well
/// inside Claude Code's 120 s Bash limit.
pub fn budget_for(stage: &Stage) -> Duration {
    let timeout = stage.effective_subagent_timeout_secs();
    let seconds = if timeout == 0 {
        100
    } else {
        (3 * timeout).saturating_sub(60).min(100)
    };
    Duration::from_secs(seconds.max(30))
}

/// Request the job, or follow the one this session already requested.
/// `relay` emits the ticket and returns its request id; the
/// `Host job: requested` lines are written to `out` before it runs, so the
/// relay line stays the last line of stdout.
pub fn wait_or_request(
    site: &WaitSite<'_>,
    relay: &mut dyn FnMut(&HostJobRequest) -> Result<String>,
    out: &mut dyn Write,
) -> Result<WaitOutcome> {
    let head = head(site.checkout)?;
    let digest = if site.knowledge {
        knowledge_tree_digest(site.checkout)?
    } else {
        tree_digest(site.checkout)?
    };
    let path = state_path(site)?;
    let state = read_state(&path).filter(|state| {
        state.head == head && state.tree_digest == digest && state.no_cache == site.no_cache
    });
    let Some(state) = state else {
        return request(site, &path, relay, out, head, digest);
    };
    let outcome = follow(site, &state)?;
    if !matches!(outcome, WaitOutcome::Pending { .. }) {
        remove_state_if_owned(&path, &state.request_id);
    }
    report(site, &head, &outcome, out)?;
    Ok(outcome)
}

fn request(
    site: &WaitSite<'_>,
    path: &Path,
    relay: &mut dyn FnMut(&HostJobRequest) -> Result<String>,
    out: &mut dyn Write,
    head: String,
    tree_digest: String,
) -> Result<WaitOutcome> {
    writeln!(out, "{}", first_line(site, "requested", &head))?;
    writeln!(
        out,
        "This is not a result. Run the same command again in your next Bash call to wait for it."
    )?;
    out.flush()?;
    let request_id = relay(&HostJobRequest {
        head: head.clone(),
        tree_digest: tree_digest.clone(),
        no_cache: site.no_cache,
    })?;
    write_state(
        path,
        &RequestState {
            request_id,
            head,
            tree_digest,
            no_cache: site.no_cache,
            submitted_at: Utc::now(),
        },
    )?;
    Ok(WaitOutcome::Requested)
}

/// Poll until the job is decided or the budget ends.
fn follow(site: &WaitSite<'_>, state: &RequestState) -> Result<WaitOutcome> {
    let end = Instant::now() + site.budget;
    loop {
        match poll(site, state)? {
            Poll::Decided(outcome) => return Ok(*outcome),
            Poll::Waiting { done } if Instant::now() >= end => {
                return Ok(WaitOutcome::Pending { done });
            }
            Poll::Waiting { .. } => {
                std::thread::sleep(POLL.min(end.saturating_duration_since(Instant::now())));
            }
        }
    }
}

enum Poll {
    Decided(Box<WaitOutcome>),
    Waiting { done: usize },
}

fn poll(site: &WaitSite<'_>, state: &RequestState) -> Result<Poll> {
    let applied = match ledger_outcome(site, &state.request_id)? {
        Some((LedgerOutcome::Applied, note)) => job_id_of(&note),
        Some((_, reason)) => return Ok(Poll::Decided(Box::new(WaitOutcome::Refused(reason)))),
        None => None,
    };
    let Some(record) = find_job(site, state, applied.as_deref())? else {
        let waited = Utc::now().signed_duration_since(state.submitted_at);
        let never = applied.is_none() && waited.to_std().is_ok_and(|age| age > REQUEST_GRACE);
        return Ok(if never {
            Poll::Decided(Box::new(WaitOutcome::Refused(
                "the request never reached the daemon".to_string(),
            )))
        } else {
            Poll::Waiting { done: 0 }
        });
    };
    if !record.state.is_terminal() {
        let done = read_progress(site.work_dir, site.stage_id, &record.id)?.len();
        return Ok(Poll::Waiting { done });
    }
    let result = load_result(site.work_dir, site.stage_id, &record.id)?;
    Ok(Poll::Decided(Box::new(WaitOutcome::Finished(
        Box::new(record),
        result,
    ))))
}

/// The session's ledger outcome for `request_id`, with its note or reason.
fn ledger_outcome(
    site: &WaitSite<'_>,
    request_id: &str,
) -> Result<Option<(LedgerOutcome, String)>> {
    let ledger = read_ledger(site.work_dir, site.session_id)?;
    let settled = ledger
        .into_iter()
        .rev()
        .find(|record| record.id == request_id && record.outcome.is_some());
    Ok(settled.and_then(|record| {
        let reason = record.reason.unwrap_or_default();
        record.outcome.map(|outcome| match outcome {
            LedgerOutcome::UnknownAfterRestart => (
                outcome,
                "the daemon restarted before the request was applied".to_string(),
            ),
            _ => (outcome, reason),
        })
    }))
}

/// `host job <id> ...` names the job a request became.
fn job_id_of(note: &str) -> Option<String> {
    let rest = note.strip_prefix("host job ")?;
    rest.split_whitespace().next().map(str::to_string)
}

/// The job record: `current.json` when it is this request's job, else the
/// job's own copy; a job whose directory was pruned reads as cancelled.
fn find_job(
    site: &WaitSite<'_>,
    state: &RequestState,
    applied: Option<&str>,
) -> Result<Option<JobRecord>> {
    let (work_dir, stage_id) = (site.work_dir, site.stage_id);
    if let Some(current) = load_current(work_dir, stage_id)? {
        let named =
            applied == Some(current.id.as_str()) || current.requests.contains(&state.request_id);
        if named {
            return Ok(Some(current));
        }
    }
    let Some(job_id) = applied else {
        return Ok(None);
    };
    Ok(Some(match load_job_record(work_dir, stage_id, job_id)? {
        Some(record) => record,
        None => pruned(site, state, job_id),
    }))
}

fn pruned(site: &WaitSite<'_>, state: &RequestState, job_id: &str) -> JobRecord {
    JobRecord {
        id: job_id.to_string(),
        stage_id: site.stage_id.to_string(),
        session_id: site.session_id.to_string(),
        kind: site.kind,
        head: state.head.clone(),
        tree_digest: state.tree_digest.clone(),
        no_cache: state.no_cache,
        state: JobState::Cancelled,
        requests: vec![state.request_id.clone()],
        queued_at: state.submitted_at,
        started_at: None,
        finished_at: None,
        pid: None,
        detail: Some("the job's record was pruned".to_string()),
    }
}

fn first_line(site: &WaitSite<'_>, state: &str, head: &str) -> String {
    let short: String = head.chars().take(SHORT_HEAD).collect();
    format!("Host job: {state} ({}, {short})", kind_name(site.kind))
}

fn kind_name(kind: JobKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Print the outcome of a followed job: its first line, then the detail.
fn report(
    site: &WaitSite<'_>,
    head: &str,
    outcome: &WaitOutcome,
    out: &mut dyn Write,
) -> Result<()> {
    let text = match outcome {
        WaitOutcome::Requested => return Ok(()),
        WaitOutcome::Refused(reason) => {
            format!("{}\n{reason}\n", first_line(site, "refused", head))
        }
        WaitOutcome::Pending { done } => format!(
            "{}: {done} criteria done. Run the same command again to keep waiting.\n",
            first_line(site, "pending", head)
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
    let mut text = format!("{}\n", first_line(site, word, head));
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

fn state_path(site: &WaitSite<'_>) -> Result<PathBuf> {
    crate::validation::validate_id(site.stage_id)?;
    let name = format!("{}-{}.json", site.stage_id, kind_name(site.kind));
    Ok(site.scratch_dir.join("host-verify").join(name))
}

fn read_state(path: &Path) -> Option<RequestState> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Atomic and private: a temp file in the same directory, renamed over.
fn write_state(path: &Path, state: &RequestState) -> Result<()> {
    let dir = path.parent().context("request state has no directory")?;
    std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(&serde_json::to_vec(state)?)?;
    file.persist(path)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// Delete the state only while it still holds `request_id`: two verifier
/// subagents of one session share the file, and one waiter's end must not
/// discard a newer request another wrote.
fn remove_state_if_owned(path: &Path, request_id: &str) {
    if read_state(path).is_some_and(|state| state.request_id == request_id) {
        let _ = std::fs::remove_file(path);
    }
}
