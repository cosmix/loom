//! `loom subagents` - a read-only watchdog over Claude Code's per-subagent
//! transcripts.
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
//! This command is strictly read-only: it never writes to a transcript or to
//! `.work/`. See `resolve` for how the transcript directory is located,
//! `classify` for how a transcript's last entry maps to a liveness state,
//! and `render` for the three subcommands themselves.

mod classify;
mod render;
// `pub(crate)` rather than private: `commands::usage` reuses this module's
// transcript-layout rules (`list_agent_files`, `agent_id_from_path`) so the two
// commands cannot disagree about where Claude Code keeps its transcripts.
pub(crate) mod resolve;

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

    /// Poll until every subagent is done, or a timeout elapses
    Watch {
        /// Seconds to poll before giving up (exit 2 on timeout)
        #[arg(long, default_value_t = 300)]
        timeout: u64,

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
}

/// Dispatch to the requested view. Every path here is read-only.
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
            timeout,
            session,
            dir,
            debounce,
        } => render::watch(timeout, session, dir, debounce),
    }
}
