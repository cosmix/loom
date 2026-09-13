//! Implements `loom subagents list|harvest|watch`. Read-only: every path
//! here only reads transcript files and prints; nothing is ever written.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;

use super::classify::{self, SubagentState, SubagentSummary};
use super::forward_jobs;
use super::resolve::{self, Resolution};
use super::table;

#[path = "render_forward.rs"]
mod forward;

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The result of resolving a transcript directory and reading every
/// subagent transcript found in it.
enum Gathered {
    /// Resolution failed outright; the string names what was looked for.
    NotFound(String),
    /// A directory was resolved and scanned (it may hold zero transcripts).
    Found(Vec<SubagentSummary>),
}

/// Best-effort state directory root for the authoritative lifecycle and spawn-ledger
/// paths in `classify::analyze` (see its doc). `None` when no state directory is
/// found -- this command has no other reason to need one, so absence is
/// silent, not an error.
fn find_work_dir_quietly() -> Option<PathBuf> {
    crate::commands::common::work_dir_path().ok()
}

/// Resolve the transcript directory and classify every subagent transcript
/// found there. Shared by `list`, `harvest`, and each `watch` poll. A
/// transcript that fails to read (not "fails to parse" -- that degrades to
/// `Unknown` inside `classify::analyze`) is reported and skipped rather
/// than aborting the whole listing. The configured ceiling is resolved once
/// by the command and reused across all transcripts (and every watch poll).
fn gather(
    session: &Option<String>,
    dir: &Option<PathBuf>,
    debounce_secs: u64,
    work_dir: Option<&Path>,
    subagent_ceiling_tokens: u64,
) -> Gathered {
    // Each index is replayed once for this complete list/watch gather cycle.
    let lifecycle = classify::lifecycle::load_active(work_dir);
    let forward_index = work_dir.and_then(load_forward_index);
    gather_with_index(
        session,
        dir,
        debounce_secs,
        work_dir,
        subagent_ceiling_tokens,
        lifecycle.as_ref(),
        forward_index.as_ref(),
    )
}

fn load_forward_index(work_dir: &Path) -> Option<forward_jobs::ForwardIndex> {
    let stage_id = std::env::var("LOOM_STAGE_ID").ok()?;
    let loom_session_id = std::env::var("LOOM_SESSION_ID").ok();
    Some(forward_jobs::load_forward_index(
        work_dir,
        &stage_id,
        loom_session_id.as_deref(),
    ))
}

fn gather_with_index(
    session: &Option<String>,
    dir: &Option<PathBuf>,
    debounce_secs: u64,
    work_dir: Option<&Path>,
    subagent_ceiling_tokens: u64,
    lifecycle: Option<&classify::lifecycle::Context>,
    forward_index: Option<&forward_jobs::ForwardIndex>,
) -> Gathered {
    match resolve::resolve(dir.clone(), session.clone()) {
        Resolution::NotFound(looked_for) => missing_gathered(looked_for, forward_index, work_dir),
        Resolution::Found(subagents_dir) => {
            let mut summaries = gather_transcripts(
                &subagents_dir,
                debounce_secs,
                work_dir,
                subagent_ceiling_tokens,
                lifecycle,
                forward_index,
            );
            forward::append_missing_summaries(&mut summaries, forward_index, work_dir);
            Gathered::Found(summaries)
        }
    }
}

fn missing_gathered(
    looked_for: String,
    forward_index: Option<&forward_jobs::ForwardIndex>,
    work_dir: Option<&Path>,
) -> Gathered {
    let mut summaries = Vec::new();
    forward::append_missing_summaries(&mut summaries, forward_index, work_dir);
    if summaries.is_empty() {
        Gathered::NotFound(looked_for)
    } else {
        Gathered::Found(summaries)
    }
}

fn gather_transcripts(
    subagents_dir: &Path,
    debounce_secs: u64,
    work_dir: Option<&Path>,
    subagent_ceiling_tokens: u64,
    lifecycle: Option<&classify::lifecycle::Context>,
    forward_index: Option<&forward_jobs::ForwardIndex>,
) -> Vec<SubagentSummary> {
    let files = resolve::list_agent_files(subagents_dir);
    let mut summaries = Vec::with_capacity(files.len());
    for path in files {
        let agent_id = resolve::agent_id_from_path(&path);
        match gather_transcript(
            &path,
            agent_id.clone(),
            debounce_secs,
            work_dir,
            subagent_ceiling_tokens,
            lifecycle,
            forward_index,
        ) {
            Ok(summary) => summaries.push(summary),
            Err(error) => eprintln!("warning: could not read {agent_id} ({error})"),
        }
    }
    summaries
}

fn gather_transcript(
    path: &Path,
    agent_id: String,
    debounce_secs: u64,
    work_dir: Option<&Path>,
    subagent_ceiling_tokens: u64,
    lifecycle: Option<&classify::lifecycle::Context>,
    forward_index: Option<&forward_jobs::ForwardIndex>,
) -> Result<SubagentSummary> {
    classify::analyze_with_evidence_at_ceiling(
        path,
        agent_id,
        debounce_secs,
        work_dir,
        subagent_ceiling_tokens,
        lifecycle,
        forward_index,
    )
}

/// `loom subagents list` -- table (or `--json`) of every subagent found.
/// Always exits 0: absence of subagents is normal, not an error.
pub fn list(
    session: Option<String>,
    dir: Option<PathBuf>,
    json: bool,
    debounce: u64,
) -> Result<()> {
    let work_dir = find_work_dir_quietly();
    let ceiling = classify::resolve_subagent_ceiling(work_dir.as_deref());
    let summaries = match gather(&session, &dir, debounce, work_dir.as_deref(), ceiling) {
        Gathered::NotFound(looked_for) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Vec::<SubagentSummary>::new())?
                );
            } else {
                println!("no subagents found: {looked_for}");
            }
            return Ok(());
        }
        Gathered::Found(summaries) => summaries,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }

    if summaries.is_empty() {
        println!("no subagent transcripts found");
        return Ok(());
    }

    table::print_table(&summaries);
    Ok(())
}

/// `loom subagents harvest` -- prints the final report text of every `done`
/// subagent (or just `--id` if given). Always exits 0: this recovers
/// reports the harness never delivered, it does not require any to exist.
///
/// A subagent that is structurally text-only but hasn't cleared the `done`
/// debounce has `final_report == None` (see `classify::analyze`), so it is
/// silently skipped here rather than harvested -- harvesting a
/// partially-flushed turn would hand the caller a truncated report that
/// looks complete.
pub fn harvest(
    id: Option<String>,
    session: Option<String>,
    dir: Option<PathBuf>,
    debounce: u64,
) -> Result<()> {
    let work_dir = find_work_dir_quietly();
    let ceiling = classify::resolve_subagent_ceiling(work_dir.as_deref());
    let summaries = match gather(&session, &dir, debounce, work_dir.as_deref(), ceiling) {
        Gathered::NotFound(looked_for) => {
            println!("no subagents found: {looked_for}");
            return Ok(());
        }
        Gathered::Found(summaries) => summaries,
    };

    let mut harvested = 0usize;
    for summary in &summaries {
        if let Some(wanted) = &id {
            if &summary.agent_id != wanted {
                continue;
            }
        }
        if print_terminal_failure(summary) {
            harvested += 1;
            continue;
        }
        let Some(report) = &summary.final_report else {
            continue;
        };
        println!("===== {} =====", summary.agent_id);
        if is_forward_state(summary.state) {
            println!("forward state: {}", summary.state.label());
        }
        println!("{report}");
        println!();
        harvested += 1;
    }

    if harvested == 0 {
        match &id {
            Some(wanted) => println!(
                "nothing harvestable for agent '{wanted}' (not found, or its turn hasn't ended yet)"
            ),
            None => println!("nothing harvestable: no subagent has finished its turn yet"),
        }
    }

    Ok(())
}

fn print_terminal_failure(summary: &SubagentSummary) -> bool {
    let Some(evidence) = terminal_failure_evidence(summary) else {
        return false;
    };
    println!("===== {} =====", summary.agent_id);
    println!("{evidence}");
    if let Some(report) = &summary.final_report {
        println!("{report}");
    }
    println!();
    true
}

fn terminal_failure_evidence(summary: &SubagentSummary) -> Option<String> {
    matches!(
        summary.state,
        SubagentState::Failed | SubagentState::Cancelled
    )
    .then(|| {
        format!(
            "terminal failure evidence: agent={} state={} reason={}",
            summary.agent_id,
            summary.state.label(),
            summary.terminal_reason.as_deref().unwrap_or("not recorded")
        )
    })
}

fn is_forward_state(state: SubagentState) -> bool {
    matches!(
        state,
        SubagentState::ForwardWait | SubagentState::ForwardFailed | SubagentState::ForwardUnknown
    )
}

/// `loom subagents watch` -- polls every 2s until either every subagent is
/// done (exit 0) or `timeout_secs` elapses (exit 2). Prints which branch
/// fired, then the current table, on both branches; never blocks past the
/// deadline and never exits silently.
///
/// Outside a Loom stage, a resolution failure means there is nothing to wait
/// for. Inside a stage it remains pending because absence is not lifecycle
/// success evidence. An empty resolved directory likewise remains pending.
///
/// Outside a stage, settlement retains the transcript debounce rule. Inside
/// a stage, even a debounced `legacy-done` row stays pending until exact
/// lifecycle success exists for every worker.
pub fn watch(
    timeout_secs: u64,
    session: Option<String>,
    dir: Option<PathBuf>,
    debounce: u64,
) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    // Resolved once, not per-poll: it's a directory path, not its contents
    // -- lifecycle and ledger reads inside classify still happen fresh every
    // poll.
    let work_dir = find_work_dir_quietly();
    let ceiling = classify::resolve_subagent_ceiling(work_dir.as_deref());
    let lifecycle_required = classify::lifecycle::stage_owned();

    loop {
        match gather(&session, &dir, debounce, work_dir.as_deref(), ceiling) {
            Gathered::NotFound(looked_for) if !lifecycle_required => {
                println!("settled: no subagents found ({looked_for})");
                return Ok(());
            }
            Gathered::NotFound(_) => {}
            Gathered::Found(summaries) => {
                let outcome = forward::watch_outcome(&summaries, lifecycle_required);
                if handle_watch_outcome(outcome, &summaries, lifecycle_required) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    exit_watch_timeout(timeout_secs, Some(&summaries));
                }
            }
        }
        if Instant::now() >= deadline {
            exit_watch_timeout(timeout_secs, None);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn handle_watch_outcome(
    outcome: forward::WatchOutcome,
    summaries: &[SubagentSummary],
    lifecycle_required: bool,
) -> bool {
    let message = match outcome {
        forward::WatchOutcome::Settled if lifecycle_required => {
            "settled: every owned worker has lifecycle success evidence"
        }
        forward::WatchOutcome::Settled => {
            "settled: legacy transcript evidence marks every subagent done"
        }
        forward::WatchOutcome::Failed => "failed: a worker or forwarded job failed",
        forward::WatchOutcome::Cancelled => "cancelled: a worker was cancelled",
        forward::WatchOutcome::Pending => return false,
    };
    println!("{message}");
    table::print_table(summaries);
    if let Some(code) = forward::exit_code(outcome, false).filter(|code| *code != 0) {
        std::process::exit(code);
    }
    true
}

fn exit_watch_timeout(timeout_secs: u64, summaries: Option<&[SubagentSummary]>) -> ! {
    println!("timeout: {timeout_secs}s elapsed without exact worker evidence");
    if let Some(summaries) = summaries {
        table::print_table(summaries);
    }
    let code = forward::exit_code(forward::WatchOutcome::Pending, true).unwrap_or(2);
    std::process::exit(code)
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "render_forward_tests.rs"]
mod forward_tests;
