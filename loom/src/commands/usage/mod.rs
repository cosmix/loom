//! `loom usage` - a read-only analyser over Claude Code's own JSONL
//! transcripts, reporting what agent sessions actually consume, in tokens.
//!
//! Loom spawns and orchestrates Claude Code sessions but has no view into
//! what they cost -- this command reads the same transcripts `loom
//! subagents` watches and turns them into a token report. The unit
//! throughout is tokens, never a price: transcripts don't carry billing
//! rates, and baking in a stale rate table would make the report wrong in a
//! way a reader can't detect. `transcript::parse` handles the two traps in
//! the source data before this module ever sees a token count: one API
//! response is written across several JSONL lines sharing a `message.id`
//! (summed once, not once per line), and a transcript's last line may be a
//! partial write (dropped rather than mis-parsed).

mod accounting;
mod discovery;
mod json;
mod sections;
mod transcript;
// `pub(crate)`, not private: `transcript_types::SYNTHETIC_MODEL` is the
// canonical model-sentinel constant shared with `commands::subagents`, a
// sibling module tree that a plain `mod` declaration would not reach.
pub(crate) mod transcript_types;

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Arguments for `loom usage`. Lives here rather than in the CLI enum so the
/// command owns its own surface - the same reasoning `SubagentsArgs` documents.
#[derive(Debug, clap::Args)]
pub struct UsageArgs {
    /// How far back to look: a duration (`7d`, `24h`, `30m`) or an ISO date (`2026-08-01`)
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Project directory to read transcripts for (defaults to this repository
    /// plus each of its `.worktrees/*` subdirectories)
    #[arg(long, conflicts_with = "all")]
    pub project: Option<PathBuf>,

    /// Read every project under ~/.claude/projects/
    #[arg(long)]
    pub all: bool,

    /// Only transcripts belonging to this stage
    #[arg(long)]
    pub stage: Option<String>,

    /// Only transcripts belonging to this plan
    #[arg(long)]
    pub plan: Option<String>,

    /// Which windowing to report the three accountings under
    #[arg(long, value_enum, default_value_t = accounting::Windowing::FiveHour)]
    pub windows: accounting::Windowing,

    /// Emit machine-readable JSON instead of the table report
    #[arg(long)]
    pub json: bool,
}

/// Parses every discovered transcript, warning on and skipping any file that
/// fails to parse rather than failing the whole report.
fn parse_all(
    files: &[discovery::DiscoveredFile],
    since: chrono::DateTime<chrono::Utc>,
    work_dir: Option<&Path>,
) -> Vec<transcript::Transcript> {
    let started_agent_types =
        crate::commands::subagents::ledger::StartedAgentTypeIndex::load(work_dir);
    let mut transcripts = Vec::with_capacity(files.len());
    for file in files {
        let agent_type = file
            .agent_id
            .as_deref()
            .and_then(|agent_id| started_agent_types.get(agent_id, &file.session_id));
        match transcript::parse(file, since) {
            Ok(mut transcript) => {
                transcript.agent_type = agent_type;
                transcripts.push(transcript);
            }
            Err(error) => eprintln!("loom usage: skipping {}: {error:#}", file.path.display()),
        }
    }
    transcripts
}

/// Resolve the optional hook ledger for the project being reported. An
/// explicit project must use its own `.work`; falling back to the caller's
/// repository would silently attach unrelated spawn metadata. `--all`
/// likewise spans repositories and therefore has no single safe ledger.
fn usage_work_dir(project: Option<&Path>, all: bool) -> Option<PathBuf> {
    if all {
        return None;
    }
    match project {
        Some(project) => {
            let project = if project.is_absolute() {
                project.to_path_buf()
            } else {
                std::env::current_dir().ok()?.join(project)
            };
            let candidate = project.join(".work");
            candidate.is_dir().then_some(candidate)
        }
        None => crate::commands::common::find_work_dir().ok(),
    }
}

pub fn execute(args: UsageArgs) -> Result<()> {
    let since = discovery::parse_since(&args.since)?;
    let work_dir = usage_work_dir(args.project.as_deref(), args.all);
    let options = discovery::DiscoveryOptions {
        since,
        project: args.project,
        all: args.all,
        stage: args.stage,
        plan: args.plan,
    };

    let files = discovery::discover(&options)?;
    let transcripts = parse_all(&files, since, work_dir.as_deref());
    let report = sections::build(&transcripts, args.windows);

    if args.json {
        json::print(&report)
    } else {
        sections::render(&report);
        Ok(())
    }
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
