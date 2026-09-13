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
//! (the last complete usage vector is counted once), and a transcript's last line may be a
//! partial write (dropped rather than mis-parsed).

mod accounting;
mod claude_provider;
mod claude_usage;
mod codex_discovery;
mod codex_provider;
mod discovery;
mod json;
mod provider;
mod provider_normalization;
mod provider_report;
mod provider_types;
mod quota_history;
mod receipt_provider;
mod sections;
mod time_range;
mod transcript;
mod transcript_content;
// `pub(crate)`, not private: `transcript_types::SYNTHETIC_MODEL` is the
// canonical model-sentinel constant shared with `commands::subagents`, a
// sibling module tree that a plain `mod` declaration would not reach.
pub(crate) mod transcript_types;

use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSelection {
    Claude,
    Codex,
    All,
}

impl ProviderSelection {
    fn includes(self, provider: provider_types::Provider) -> bool {
        matches!(self, Self::All)
            || matches!(
                (self, provider),
                (Self::Claude, provider_types::Provider::Claude)
                    | (Self::Codex, provider_types::Provider::Codex)
            )
    }
}

/// Arguments for `loom usage`. Lives here rather than in the CLI enum so the
/// command owns its own surface - the same reasoning `SubagentsArgs` documents.
#[derive(Debug, clap::Args)]
pub struct UsageArgs {
    /// How far back to look: a duration (`7d`, `24h`, `30m`) or an ISO date (`2026-08-01`)
    #[arg(long, default_value = "7d")]
    pub since: String,

    /// Inclusive UTC RFC3339 upper bound for event timestamps
    #[arg(long)]
    pub until: Option<String>,

    /// Provider telemetry to normalize
    #[arg(long, value_enum, default_value_t = ProviderSelection::Claude)]
    pub provider: ProviderSelection,

    /// Explicit Claude projects root; never falls back when supplied
    #[arg(long)]
    pub claude_root: Option<PathBuf>,

    /// Explicit Codex root containing sessions/ and archived_sessions/
    #[arg(long)]
    pub codex_root: Option<PathBuf>,

    /// Explicit directory containing execution receipt protocol files
    #[arg(long)]
    pub receipts_root: Option<PathBuf>,

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
    range: &time_range::TimeRange,
    work_dir: Option<&Path>,
) -> Vec<transcript::Transcript> {
    let started_agent_types =
        crate::commands::subagents::ledger::StartedAgentTypeIndex::load(work_dir);
    let mut transcripts = Vec::with_capacity(files.len());
    for file in files {
        let metadata = file
            .agent_id
            .as_deref()
            .and_then(|agent_id| started_agent_types.get_metadata(agent_id, &file.session_id));
        match transcript::parse(file, range) {
            Ok(mut transcript) => {
                if let Some(metadata) = metadata {
                    transcript.agent_type = Some(metadata.agent_type);
                    transcript.stage_id = metadata.stage_id;
                    transcript.loom_session_id = metadata.loom_session_id;
                }
                transcripts.push(transcript);
            }
            Err(error) => eprintln!("loom usage: skipping {}: {error:#}", file.path.display()),
        }
    }
    transcripts
}

/// Resolve the optional hook ledger for the project being reported. An
/// explicit project must use its own state directory; falling back to the
/// caller's repository would silently attach unrelated spawn metadata.
/// `--all` likewise spans repositories and therefore has no single safe
/// ledger.
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
            let candidate = crate::fs::work_dir::WorkDir::new(&project)
                .ok()?
                .root()
                .to_path_buf();
            candidate.is_dir().then_some(candidate)
        }
        None => crate::commands::common::work_dir_path().ok(),
    }
}

pub fn execute(args: UsageArgs) -> Result<()> {
    let range =
        time_range::TimeRange::parse(&args.since, args.until.as_deref(), chrono::Utc::now())?;
    let work_dir = usage_work_dir(args.project.as_deref(), args.all);
    let mut normalized = normalize_usage(&args, range, work_dir.as_deref())?;
    attach_quota_history(
        &mut normalized.ledger,
        args.provider,
        work_dir.as_deref(),
        range,
    );
    let report = sections::build(
        &normalized.claude_transcripts,
        args.windows,
        normalized.ledger,
    );

    render_usage_report(&report, args.json)
}

fn normalize_usage(
    args: &UsageArgs,
    range: time_range::TimeRange,
    work_dir: Option<&Path>,
) -> Result<provider::NormalizedProviderEvents> {
    let options = discovery::DiscoveryOptions {
        range,
        claude_root: args.claude_root.clone(),
        project: args.project.clone(),
        all: args.all,
        stage: args.stage.clone(),
        plan: args.plan.clone(),
    };
    let claude_missing_roots = usize::from(
        args.provider.includes(provider_types::Provider::Claude)
            && discovery::root_is_missing(&options),
    );
    let files = if args.provider.includes(provider_types::Provider::Claude) {
        discovery::discover(&options)?
    } else {
        Vec::new()
    };
    let transcripts = parse_all(&files, &range, work_dir);
    let codex = if args.provider.includes(provider_types::Provider::Codex) {
        codex_discovery::discover(args.codex_root.as_deref())
    } else {
        codex_discovery::CodexDiscovery::default()
    };
    Ok(provider::normalize_provider_events(
        provider::ProviderEventInput {
            selection: args.provider,
            range,
            claude_transcripts: transcripts,
            claude_discovered_files: files.len(),
            claude_missing_roots,
            codex_files: &codex.files,
            codex_missing_roots: codex.missing_roots,
            codex_unreadable_directories: codex.unreadable_directories,
            receipts_root: args.receipts_root.as_deref(),
        },
    ))
}

fn attach_quota_history(
    ledger: &mut provider_types::ProviderLedger,
    selection: ProviderSelection,
    work_root: Option<&Path>,
    range: time_range::TimeRange,
) {
    let providers = [
        provider_types::Provider::Claude,
        provider_types::Provider::Codex,
    ]
    .into_iter()
    .filter(|provider| selection.includes(*provider))
    .map(|provider| provider_quota_history(provider, work_root, range))
    .collect();
    ledger.quota_history = Some(quota_history::QuotaHistorySection::new(providers));
}

fn provider_quota_history(
    provider: provider_types::Provider,
    work_root: Option<&Path>,
    range: time_range::TimeRange,
) -> quota_history::ProviderQuotaHistory {
    let Some(work_root) = work_root else {
        return quota_history::ProviderQuotaHistory::unavailable(provider);
    };
    let history = crate::quota::read_history(
        work_root,
        provider.name(),
        range.since.timestamp(),
        range.until.map(|until| until.timestamp()),
    );
    quota_history::ProviderQuotaHistory::from_read(provider, history)
}

fn render_usage_report(report: &sections::Report, json_output: bool) -> Result<()> {
    if json_output {
        return json::print(report);
    }
    sections::render(report);
    Ok(())
}

#[cfg(test)]
#[path = "usage_tests.rs"]
mod tests;
