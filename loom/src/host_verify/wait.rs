//! The session side of a host job: request one, then follow it across Bash
//! calls. A call that finds no matching request state relays a ticket and
//! returns at once; a call that finds one waits for the job inside its budget.
//! Both exit 0 while nothing is decided, because Claude Code runs the relay
//! and heartbeat hooks only after a Bash call that exits 0.

use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;

use super::digest::{head, knowledge_tree_digest, tree_digest};
use super::store::{load_result, read_progress};
use super::wait_report::report;
use super::wait_state::{
    find_job, job_id_of, ledger_outcome, read_state, remove_state_if_owned, state_path,
    write_state, RequestState,
};
use super::{JobKind, JobRecord, JobResult, JobState};
use crate::fs::inbox::LedgerOutcome;
use crate::models::stage::Stage;
use crate::relay::emit::{RelaySink, StdSink};
use crate::relay::HostJobRequest;

pub use super::wait_report::{head_or_empty, heading, refusal};

/// How long a request may sit with no ledger outcome and no job naming it
/// before the call reports that it never reached the daemon.
const REQUEST_GRACE: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_secs(1);

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

/// A relay sink that holds the ticket's `LOOM_RELAY_V1` line back from stdout
/// until [`TicketSink::print_line`]. The caller prints it once
/// [`wait_or_request`] returned `Requested`: the request state is on disk by
/// then, the relay line is the last line of stdout, and a call that failed
/// shows no ticket the hook would never relay.
#[derive(Default)]
pub struct TicketSink {
    line: Vec<u8>,
    std: StdSink,
}

impl RelaySink for TicketSink {
    fn stdout(&mut self) -> &mut dyn Write {
        &mut self.line
    }

    fn stderr(&mut self) -> &mut dyn Write {
        self.std.stderr()
    }
}

impl TicketSink {
    /// Write the held relay line to `out`, once.
    pub fn print_line(&mut self, out: &mut dyn Write) -> Result<()> {
        out.write_all(&std::mem::take(&mut self.line))?;
        out.flush()?;
        Ok(())
    }
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
/// `relay` emits the ticket into a [`TicketSink`] and returns its request id;
/// the `Host job: requested` lines are written to `out` once the ticket is
/// emitted and its state recorded. Any failure ends as `Refused` with its
/// `Host job: refused` first line, never a bare error, so a session always
/// has a state to act on.
pub fn wait_or_request(
    site: &WaitSite<'_>,
    relay: &mut dyn FnMut(&HostJobRequest) -> Result<String>,
    out: &mut dyn Write,
) -> Result<WaitOutcome> {
    match drive(site, relay, out) {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            let message = format!("{error:#}");
            refusal(out, site.kind, &head_or_empty(site.checkout), &message)?;
            Ok(WaitOutcome::Refused(message))
        }
    }
}

fn drive(
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
    if awaits_application(site, &outcome) {
        // The caller follows the application, reports it, then `release`s.
        return Ok(outcome);
    }
    if !matches!(outcome, WaitOutcome::Pending { .. }) {
        remove_state_if_owned(&path, &state.request_id);
    }
    report(site, &head, &outcome, out)?;
    Ok(outcome)
}

/// A passed completion is decided only once the daemon applies it, so its
/// request outlives the job.
fn awaits_application(site: &WaitSite<'_>, outcome: &WaitOutcome) -> bool {
    site.kind == JobKind::Complete
        && matches!(outcome, WaitOutcome::Finished(record, _) if record.state == JobState::Passed)
}

/// Forget the request once the caller has reported how the daemon applied
/// `record`, the passed completion [`wait_or_request`] returned.
pub fn release(site: &WaitSite<'_>, record: &JobRecord) -> Result<()> {
    let path = state_path(site)?;
    let owned = read_state(&path).filter(|state| record.requests.contains(&state.request_id));
    if let Some(state) = owned {
        remove_state_if_owned(&path, &state.request_id);
    }
    Ok(())
}

/// The ticket, then its state, then the request's lines. The lines come last
/// so a request that failed prints `Host job: refused` first, never a
/// `requested` state that a rerun would meet again; the ticket's relay line
/// stays in the caller's sink, so a state that cannot be written withholds it.
fn request(
    site: &WaitSite<'_>,
    path: &Path,
    relay: &mut dyn FnMut(&HostJobRequest) -> Result<String>,
    out: &mut dyn Write,
    head: String,
    tree_digest: String,
) -> Result<WaitOutcome> {
    let request_id = relay(&HostJobRequest {
        head: head.clone(),
        tree_digest: tree_digest.clone(),
        no_cache: site.no_cache,
    })
    .context("the request was not sent")?;
    let state = RequestState {
        request_id,
        head: head.clone(),
        tree_digest,
        no_cache: site.no_cache,
        submitted_at: Utc::now(),
    };
    write_state(path, &state).context("the request was not sent")?;
    writeln!(out, "{}", heading(site.kind, "requested", &head))?;
    writeln!(
        out,
        "This is not a result. Run the same command again in your next Bash call to wait for it."
    )?;
    out.flush()?;
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
