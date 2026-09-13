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
    // Load receipt evidence before transcript resolution: an empty directory
    // or missing session cannot settle a persisted expected forward.
    let forward_index = work_dir.and_then(load_forward_index);
    gather_with_index(
        session,
        dir,
        debounce_secs,
        work_dir,
        subagent_ceiling_tokens,
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
    forward_index: Option<&forward_jobs::ForwardIndex>,
) -> Result<SubagentSummary> {
    match forward_index {
        Some(index) => classify::analyze_with_forward_at_ceiling(
            path,
            agent_id,
            debounce_secs,
            work_dir,
            subagent_ceiling_tokens,
            index,
        ),
        None => classify::analyze_at_ceiling(
            path,
            agent_id,
            debounce_secs,
            work_dir,
            subagent_ceiling_tokens,
        ),
    }
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
/// A resolution failure (no session found at all) is treated as settled
/// immediately: there is nothing to wait for. A resolved directory that is
/// merely empty right now is NOT treated as settled -- subagents may not
/// have started writing yet -- so watch keeps polling it until timeout.
///
/// "Every subagent is done" only counts entries that have cleared the
/// `done` debounce (see `classify::analyze`): a text-only entry still
/// inside the debounce window is reported as `generating`, so it correctly
/// keeps watch from declaring settled on a subagent that is still mid-turn.
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

    loop {
        match gather(&session, &dir, debounce, work_dir.as_deref(), ceiling) {
            Gathered::NotFound(looked_for) => {
                println!("settled: no subagents found ({looked_for})");
                return Ok(());
            }
            Gathered::Found(summaries) => {
                match forward::watch_outcome(&summaries) {
                    forward::WatchOutcome::Settled => {
                        println!("settled: every subagent is done");
                        table::print_table(&summaries);
                        return Ok(());
                    }
                    forward::WatchOutcome::ForwardFailed => {
                        println!("failed: a forwarded job failed or was canceled");
                        table::print_table(&summaries);
                        std::process::exit(
                            forward::exit_code(forward::WatchOutcome::ForwardFailed, false)
                                .expect("forward failure has an exit code"),
                        );
                    }
                    forward::WatchOutcome::Pending => {}
                }

                if Instant::now() >= deadline {
                    println!("timeout: {timeout_secs}s elapsed with subagents still active");
                    table::print_table(&summaries);
                    std::process::exit(
                        forward::exit_code(forward::WatchOutcome::Pending, true)
                            .expect("timed-out wait has an exit code"),
                    );
                }
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "render_forward_tests.rs"]
mod forward_tests;
