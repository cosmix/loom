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
mod args;
mod claude_provider;
mod claude_usage;
pub(crate) mod codex_discovery;
mod codex_provider;
mod codex_scope;
mod codex_tokens;
mod comparison;
mod comparison_eval;
mod comparison_latency;
mod comparison_net;
mod comparison_quality;
mod comparison_quota;
mod comparison_schema;
mod comparison_token;
mod comparison_validation;
mod completeness;
#[cfg(test)]
mod cross_surface_tests;
mod discovery;
mod forward_join;
mod json;
mod provider;
mod provider_normalization;
mod provider_report;
mod provider_types;
mod quota_history;
mod receipt_provider;
mod report_types;
mod run_evidence;
mod run_evidence_stages;
mod sections;
mod spawn_join;
mod stream_types;
mod time_range;
mod transcript;
mod transcript_content;
mod transcript_merge;
// `pub(crate)`, not private: `transcript_types::SYNTHETIC_MODEL` is the
// canonical model-sentinel constant shared with `commands::subagents`, a
// sibling module tree that a plain `mod` declaration would not reach.
pub(crate) mod transcript_types;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{ensure, Result};

pub use args::{ProviderSelection, UsageArgs};

/// Parses every discovered transcript, warning on and skipping any file that
/// fails to parse rather than failing the whole report.
fn parse_all(
    files: &[discovery::DiscoveredFile],
    range: &time_range::TimeRange,
    work_dir: Option<&Path>,
    forward_join: &forward_join::ForwardJoin,
) -> Vec<transcript::Transcript> {
    let started_agent_types =
        crate::commands::subagents::ledger::StartedAgentTypeIndex::load(work_dir);
    let requested_models = spawn_join::RequestedModels::load(work_dir);
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
                    transcript.requested_model = file
                        .agent_id
                        .as_deref()
                        .and_then(|agent_id| {
                            requested_models.requested_model(agent_id, &file.session_id)
                        })
                        .map(str::to_owned);
                }
                forward_join.join_transcript(&mut transcript);
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
        Some(project) => project_work_dir(project).filter(|candidate| candidate.is_dir()),
        None => crate::commands::common::work_dir_path().ok(),
    }
}

fn project_work_dir(project: &Path) -> Option<PathBuf> {
    let project = if project.is_absolute() {
        project.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(project)
    };
    Some(
        crate::fs::work_dir::WorkDir::new(project)
            .ok()?
            .root()
            .to_path_buf(),
    )
}

pub fn execute(args: UsageArgs) -> Result<()> {
    if let Some(artifact) = args.compare.as_deref() {
        let exit_code = comparison::compare(artifact, &args)?;
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
        return Ok(());
    }
    if let Some(declaration) = args.run_evidence.as_deref() {
        return run_evidence::produce(&args, declaration);
    }
    ensure!(
        !(args.all && args.forward_receipts_root.is_some()),
        "--forward-receipts-root cannot be used with --all"
    );
    let range =
        time_range::TimeRange::parse(args.since(), args.until.as_deref(), chrono::Utc::now())?;
    let work_dir = usage_work_dir(args.project.as_deref(), args.all);
    let mut normalized = normalize_usage(&args, args.provider(), range, work_dir.as_deref())?;
    attach_quota_history(
        &mut normalized.ledger,
        args.provider(),
        work_dir.as_deref(),
        range,
    );
    let report = sections::build(
        &normalized.claude_transcripts,
        args.windows(),
        normalized.ledger,
    );

    render_usage_report(&report, args.json)
}

fn normalize_usage(
    args: &UsageArgs,
    selection: ProviderSelection,
    range: time_range::TimeRange,
    work_dir: Option<&Path>,
) -> Result<provider::NormalizedProviderEvents> {
    let forward_root = forward_receipts_root(
        args.all,
        args.forward_receipts_root.as_deref(),
        args.project.as_deref(),
        work_dir,
    )?;
    let forward_join = forward_join::ForwardJoin::load(forward_root);
    let claude = discover_claude(args, selection, range, work_dir, &forward_join)?;
    let codex = if selection.includes(provider_types::Provider::Codex) {
        codex_discovery::discover(args.codex_root.as_deref())
    } else {
        codex_discovery::CodexDiscovery::default()
    };
    let scope = codex_scope::CodexScope::for_selection(args.all, &forward_join, &range);
    let mut normalized = provider::normalize_provider_events(provider::ProviderEventInput {
        selection,
        range,
        claude_transcripts: claude.transcripts,
        claude_discovered_files: claude.discovered,
        claude_missing_roots: claude.missing_roots,
        claude_sessions: claude.sessions,
        codex_files: &codex.files,
        codex_missing_roots: codex.missing_roots,
        codex_unreadable_directories: codex.unreadable_directories,
        codex_scope: &scope,
        receipts_root: args.receipts_root.as_deref(),
    });
    forward_join.join_ledger(&mut normalized.ledger);
    // `join_ledger` merges the receipt-load diagnostics after the reports were
    // built, so their assessment is stale until repeated.
    for report in &mut normalized.ledger.providers {
        report.completeness =
            completeness::assess(report.provider, &report.coverage, &report.diagnostics);
    }
    Ok(normalized)
}

struct ClaudeInput {
    transcripts: Vec<transcript::Transcript>,
    discovered: usize,
    missing_roots: usize,
    sessions: completeness::SessionInventory,
}

fn discover_claude(
    args: &UsageArgs,
    selection: ProviderSelection,
    range: time_range::TimeRange,
    work_dir: Option<&Path>,
    forward_join: &forward_join::ForwardJoin,
) -> Result<ClaudeInput> {
    let included = selection.includes(provider_types::Provider::Claude);
    let options = discovery::DiscoveryOptions {
        range,
        claude_root: args.claude_root.clone(),
        project: args.project.clone(),
        all: args.all,
        stage: args.stage.clone(),
        plan: args.plan.clone(),
    };
    let missing_roots = usize::from(included && discovery::root_is_missing(&options));
    let files = if included {
        discovery::discover(&options)?
    } else {
        Vec::new()
    };
    let sessions = if included && !args.all {
        let read: Vec<PathBuf> = files.iter().map(|file| file.path.clone()).collect();
        let stages = session_stage_filter(args);
        completeness::missed_claude_transcripts(work_dir, &read, &range, stages.as_ref())
    } else {
        completeness::SessionInventory::default()
    };
    Ok(ClaudeInput {
        transcripts: parse_all(&files, &range, work_dir, forward_join),
        discovered: files.len(),
        missing_roots,
        sessions,
    })
}

/// The stages whose session records count toward missed transcripts: the
/// `--stage`, the `--plan`'s stages (none when the plan cannot be resolved),
/// or every stage.
fn session_stage_filter(args: &UsageArgs) -> Option<HashSet<String>> {
    if let Some(stage) = &args.stage {
        return Some(HashSet::from([stage.clone()]));
    }
    let plan = args.plan.as_deref()?;
    Some(
        discovery::plan_stage_ids(plan)
            .ok()
            .flatten()
            .unwrap_or_default(),
    )
}

fn forward_receipts_root<'a>(
    all: bool,
    explicit: Option<&'a Path>,
    project: Option<&Path>,
    work_dir: Option<&'a Path>,
) -> Result<Option<&'a Path>> {
    if all {
        return Ok(None);
    }
    if let Some(explicit) = explicit {
        let expected = project
            .and_then(project_work_dir)
            .or_else(|| work_dir.map(Path::to_path_buf));
        ensure!(
            expected
                .as_deref()
                .is_some_and(|root| roots_match(explicit, root)),
            "--forward-receipts-root must be the selected project's Loom state root"
        );
    }
    Ok(explicit.or(work_dir))
}

fn roots_match(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => std::path::absolute(left)
            .ok()
            .zip(std::path::absolute(right).ok())
            .is_some_and(|(left, right)| left == right),
    }
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

#[cfg(test)]
#[path = "forward_root_tests.rs"]
mod forward_root_tests;

#[cfg(test)]
#[path = "comparison_tests.rs"]
mod comparison_tests;
