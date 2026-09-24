//! `loom subagents` - inspect subagents or wait once for exact owned workers.
//!
//! Claude Code appends every subagent's turn-by-turn transcript to its own
//! JSONL file under `~/.claude/projects/<slug>/<session-uuid>/subagents/`,
//! in real time, with zero cooperation required from the subagent. That
//! matters because subagents routinely finish their work, write their final
//! report, and never deliver a task notification back to the orchestrator --
//! which then waits indefinitely. Reading the transcript directly tells us
//! whether a subagent is generating, waiting on a tool, or already done, and
//! recovers its final report text even when the harness never delivered it.
//!
//! `list` and `harvest` are read-only diagnostics. `watch` writes only its
//! bounded ownership lease under the system scratch directory (`TMPDIR`),
//! never the Loom state directory or a transcript.

mod classify;
mod forward_jobs;
mod forward_jobs_transcript;
#[path = "forward_jobs_wait.rs"]
mod forward_jobs_wait;
pub(crate) mod ledger;
mod metrics;
mod render;
mod wait;
// `pub(crate)` rather than private: `commands::usage` reuses this module's
// transcript-layout rules (`list_agent_files`, `agent_id_from_path`) so the two
// commands cannot disagree about where Claude Code keeps its transcripts.
pub(crate) mod resolve;
mod summary;
mod table;

// `commands::hook::review_harvest` reads a reviewer's final text with the same
// transcript-entry rules `classify` applies.
pub(crate) use classify::{is_assistant, text_blocks};

use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;

/// Arguments for `loom subagents`. Lives here (rather than in the CLI enum)
/// so the command owns its own surface -- the same reasoning `loom map`
/// documents for `MapArgs`.
#[derive(Debug, clap::Args)]
pub struct SubagentsArgs {
    #[command(subcommand)]
    pub command: SubagentsCommand,
}

#[derive(Debug, Subcommand)]
pub enum SubagentsCommand {
    /// List every subagent transcript found, with its liveness state
    List {
        /// Session UUID to inspect (defaults to the most recently active
        /// session under this working directory's project slug)
        #[arg(long)]
        session: Option<String>,

        /// Explicit transcript directory, bypassing session/slug resolution
        #[arg(long)]
        dir: Option<PathBuf>,

        /// Emit machine-readable JSON instead of a table
        #[arg(long)]
        json: bool,

        /// Seconds a text-only, no-tool-use last entry must sit idle before
        /// it is trusted as `done` rather than mid-turn (see the `done`
        /// debounce note in `classify`)
        #[arg(long, default_value_t = classify::DEFAULT_DONE_DEBOUNCE_SECS)]
        debounce: u64,
    },

    /// Print the final report text of every subagent whose turn has ended
    Harvest {
        /// Only harvest this agent ID
        #[arg(long)]
        id: Option<String>,

        /// Session UUID to inspect (defaults to the most recently active
        /// session under this working directory's project slug)
        #[arg(long)]
        session: Option<String>,

        /// Explicit transcript directory, bypassing session/slug resolution
        #[arg(long)]
        dir: Option<PathBuf>,

        /// Seconds a text-only, no-tool-use last entry must sit idle before
        /// it is trusted as `done` rather than mid-turn (see the `done`
        /// debounce note in `classify`)
        #[arg(long, default_value_t = classify::DEFAULT_DONE_DEBOUNCE_SECS)]
        debounce: u64,
    },

    /// Wait once for an explicit set of owned workers to reach terminal state
    Watch {
        /// Worker to wait on, `claude:<agent-id>` or `codex:<unit-id>`; repeat for each
        #[arg(long = "worker")]
        workers: Vec<String>,

        /// Claude parent session UUID, only to disambiguate; never the Loom session ID
        #[arg(long)]
        session: Option<String>,

        /// Seconds until the wait deadline (exit 2); a deadline is not proof a worker died
        #[arg(long, default_value_t = 300)]
        timeout: u64,

        /// Seconds without progress before a bound worker counts as hung (exit 6);
        /// default: the stage's subagent_timeout_secs, else 600
        #[arg(long = "stall-secs")]
        stall_secs: Option<u64>,

        /// Emit one JSON object per wait event
        #[arg(long)]
        json: bool,

        /// Removed legacy transcript-directory form; retained only for migration errors
        #[arg(long, hide = true)]
        dir: Option<PathBuf>,
    },

    /// Wait for one exact forwarded job receipt without starting or changing it
    Wait {
        /// Deterministic forward receipt ID (64 lowercase hexadecimal characters)
        #[arg(long)]
        receipt: String,

        /// Seconds to poll before returning unknown (exit 2)
        #[arg(long, default_value_t = 300)]
        timeout: u64,

        /// Emit only receipt ID, backend ID, and state as JSON
        #[arg(long)]
        json: bool,
    },
}

/// Dispatch to the requested diagnostic or bounded wait.
pub fn execute(args: SubagentsArgs) -> Result<()> {
    match args.command {
        SubagentsCommand::List {
            session,
            dir,
            json,
            debounce,
        } => render::list(session, dir, json, debounce),
        SubagentsCommand::Harvest {
            id,
            session,
            dir,
            debounce,
        } => render::harvest(id, session, dir, debounce),
        SubagentsCommand::Watch {
            workers,
            session,
            timeout,
            stall_secs,
            json,
            dir,
        } => wait::run(wait::WatchRequest {
            workers,
            session,
            timeout_secs: timeout,
            stall_secs,
            json,
            legacy_dir: dir,
        }),
        SubagentsCommand::Wait {
            receipt,
            timeout,
            json,
        } => forward_jobs_wait::wait(receipt, timeout, json),
    }
}
