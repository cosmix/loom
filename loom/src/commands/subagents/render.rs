//! Implements `loom subagents list|harvest|watch`. Read-only: every path
//! here only reads transcript files and prints; nothing is ever written.

use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use colored::Colorize;

use super::classify::{self, SubagentState, SubagentSummary};
use super::resolve::{self, Resolution};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The result of resolving a transcript directory and reading every
/// subagent transcript found in it.
enum Gathered {
    /// Resolution failed outright; the string names what was looked for.
    NotFound(String),
    /// A directory was resolved and scanned (it may hold zero transcripts).
    Found(Vec<SubagentSummary>),
}

/// Best-effort `.work/` root for the authoritative-termination fast path in
/// `classify::analyze` (see its doc). `None` when no `.work/` is found --
/// this command has no other reason to need one, so absence is silent, not
/// an error.
fn find_work_dir_quietly() -> Option<PathBuf> {
    crate::commands::common::find_work_dir().ok()
}

/// Resolve the transcript directory and classify every subagent transcript
/// found there. Shared by `list`, `harvest`, and each `watch` poll. A
/// transcript that fails to read (not "fails to parse" -- that degrades to
/// `Unknown` inside `classify::analyze`) is reported and skipped rather
/// than aborting the whole listing. `debounce_secs` and `work_dir` are
/// forwarded to `classify::analyze` unchanged -- see its doc for what each
/// gates.
fn gather(
    session: &Option<String>,
    dir: &Option<PathBuf>,
    debounce_secs: u64,
    work_dir: Option<&Path>,
) -> Gathered {
    match resolve::resolve(dir.clone(), session.clone()) {
        Resolution::NotFound(looked_for) => Gathered::NotFound(looked_for),
        Resolution::Found(subagents_dir) => {
            let files = resolve::list_agent_files(&subagents_dir);
            let mut summaries = Vec::with_capacity(files.len());
            for path in files {
                let agent_id = resolve::agent_id_from_path(&path);
                match classify::analyze(&path, agent_id.clone(), debounce_secs, work_dir) {
                    Ok(summary) => summaries.push(summary),
                    Err(error) => eprintln!("warning: could not read {agent_id} ({error})"),
                }
            }
            Gathered::Found(summaries)
        }
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
    let summaries = match gather(&session, &dir, debounce, work_dir.as_deref()) {
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

    print_table(&summaries);
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
    let summaries = match gather(&session, &dir, debounce, work_dir.as_deref()) {
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
    // -- has_authoritative_termination still reads it fresh every poll.
    let work_dir = find_work_dir_quietly();

    loop {
        match gather(&session, &dir, debounce, work_dir.as_deref()) {
            Gathered::NotFound(looked_for) => {
                println!("settled: no subagents found ({looked_for})");
                return Ok(());
            }
            Gathered::Found(summaries) => {
                let settled = !summaries.is_empty()
                    && summaries.iter().all(|s| s.state == SubagentState::Done);
                if settled {
                    println!("settled: every subagent is done");
                    print_table(&summaries);
                    return Ok(());
                }

                if Instant::now() >= deadline {
                    println!("timeout: {timeout_secs}s elapsed with subagents still active");
                    print_table(&summaries);
                    std::process::exit(2);
                }
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn print_table(summaries: &[SubagentSummary]) {
    if summaries.is_empty() {
        println!("no subagent transcripts found");
        return;
    }

    // AGENT ID is the only variable-width column: real ids (user-chosen, up
    // to ~34 chars, e.g. "aexplore-doctrine-1d2b4f8be46cae08") exceed the
    // historical 28-char width, and a hardcoded width pushes every later
    // column out of alignment on any row with a long id. Compute the width
    // from the data, floored at the pre-existing 28-char width so a short
    // list still looks exactly as it did before. Ids are never truncated --
    // this is the handle a human copy-pastes into `harvest --id`.
    const MIN_AGENT_ID_WIDTH: usize = 28;
    let agent_id_width = summaries
        .iter()
        .map(|s| s.agent_id.chars().count())
        .max()
        .unwrap_or(0)
        .max(MIN_AGENT_ID_WIDTH);

    let header = format!(
        "{:<width$} {:<10} {:>9} {:>5}  {}",
        "AGENT ID",
        "STATE",
        "IDLE(S)",
        "TURNS",
        "LAST TOOL",
        width = agent_id_width
    );
    println!("{}", header.bold());
    for summary in summaries {
        println!(
            "{:<width$} {:<10} {:>9} {:>5}  {}",
            summary.agent_id,
            summary.state.label(),
            summary.idle_secs,
            summary.turns,
            summary.last_tool.as_deref().unwrap_or("-"),
            width = agent_id_width
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use classify::DEFAULT_DONE_DEBOUNCE_SECS;

    #[test]
    fn print_table_handles_empty_summaries_without_panicking() {
        print_table(&[]);
    }

    #[test]
    fn list_on_unresolvable_dir_still_succeeds() {
        // A bogus explicit --dir resolves (Resolution::Found) but reads
        // back zero files; list() must still exit 0.
        let result = list(
            None,
            Some(PathBuf::from("/nonexistent/subagents/dir")),
            false,
            DEFAULT_DONE_DEBOUNCE_SECS,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn harvest_on_empty_dir_reports_nothing_without_erroring() {
        let result = harvest(
            None,
            None,
            Some(PathBuf::from("/nonexistent/subagents/dir")),
            DEFAULT_DONE_DEBOUNCE_SECS,
        );
        assert!(result.is_ok());
    }

    /// (c) harvest must emit nothing for a subagent whose last entry looks
    /// turn-final but hasn't cleared the debounce yet. Asserted at the
    /// `gather` boundary harvest itself is built on: harvest's only print
    /// branch is gated on `final_report.is_some()`, so a summary list with
    /// no `final_report` is exactly "nothing harvestable" -- the same
    /// condition a stdout capture would be checking indirectly.
    #[test]
    fn harvest_emits_nothing_for_undebounced_text_only_entry() {
        let temp = tempfile::tempdir().unwrap();
        let content = format!(
            "{}\n",
            serde_json::json!({
                "type": "assistant",
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "message": {
                    "role": "assistant",
                    "content": [{"type": "text", "text": "still narrating"}],
                },
            })
        );
        std::fs::write(temp.path().join("agent-x.jsonl"), content).unwrap();

        let Gathered::Found(summaries) = gather(
            &None,
            &Some(temp.path().to_path_buf()),
            DEFAULT_DONE_DEBOUNCE_SECS,
            None,
        ) else {
            panic!("an explicit --dir must always resolve");
        };
        assert_eq!(summaries.len(), 1);
        assert!(
            summaries[0].final_report.is_none(),
            "a fresh text-only entry must not be harvestable yet"
        );

        // harvest() itself must still exit 0 and print only the "nothing
        // harvestable" fallback, never the report -- covered above by the
        // exhaustive if-let in harvest()'s loop, which only ever prints
        // when final_report is Some.
        let result = harvest(
            None,
            None,
            Some(temp.path().to_path_buf()),
            DEFAULT_DONE_DEBOUNCE_SECS,
        );
        assert!(result.is_ok());
    }

    /// `tool-wait` must never be harvestable and never count as settled, no
    /// matter how long it has sat idle -- a real tool call in this
    /// codebase has been measured running 603s, so a time-based rule here
    /// would misclassify a busy agent as dead.
    #[test]
    fn tool_wait_idle_30_minutes_never_harvested_and_never_settles() {
        let temp = tempfile::tempdir().unwrap();
        let old_timestamp = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        let content = format!(
            "{}\n",
            serde_json::json!({
                "type": "assistant",
                "timestamp": old_timestamp,
                "message": {
                    "role": "assistant",
                    "content": [{"type": "tool_use", "name": "Bash", "input": {}}],
                },
            })
        );
        std::fs::write(temp.path().join("agent-x.jsonl"), content).unwrap();

        let Gathered::Found(summaries) = gather(
            &None,
            &Some(temp.path().to_path_buf()),
            DEFAULT_DONE_DEBOUNCE_SECS,
            None,
        ) else {
            panic!("an explicit --dir must always resolve");
        };
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].state, SubagentState::ToolWait);
        assert!(
            summaries[0].final_report.is_none(),
            "tool-wait must never be harvestable"
        );
        // Mirrors watch()'s settled condition directly, rather than calling
        // watch() itself (which loops/sleeps in a unit test).
        let settled =
            !summaries.is_empty() && summaries.iter().all(|s| s.state == SubagentState::Done);
        assert!(
            !settled,
            "tool-wait must never be reported as settled, regardless of idle time"
        );

        let result = harvest(
            None,
            None,
            Some(temp.path().to_path_buf()),
            DEFAULT_DONE_DEBOUNCE_SECS,
        );
        assert!(result.is_ok());
    }
}
