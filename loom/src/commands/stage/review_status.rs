//! `loom stage review status`: a stage's recorded review rounds, its open
//! findings, and what changed since the latest round (DESIGN D12). The main
//! agent pastes this output into the next reviewer's brief.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::git::worktree::find_repo_root_from_cwd;
use crate::models::stage::Stage;
use crate::verify::review::fingerprint::{self, ChangeFingerprint};
use crate::verify::review::report::single_line;
use crate::verify::review::store::{self, OpenFinding, ReviewRound};
use crate::verify::transitions::load_stage;

use super::acceptance_runner::resolve_stage_execution_paths;

/// `loom stage review status <stage-id>`.
pub fn review_status(stage_id: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    let stage = load_stage(&stage_id, &work_dir)?;
    let rounds = store::load_rounds(&work_dir, &stage_id)?;
    let open = store::open_findings(&work_dir, &stage_id)?;

    print_rounds(&stage_id, &rounds);
    println!();
    print_open_findings(&open);
    println!();
    match current_fingerprint(&work_dir, &stage) {
        Ok(current) => print_freshness(rounds.last(), &current),
        Err(error) => println!(
            "Current fingerprint: unavailable ({})",
            single_line(&format!("{error:#}"))
        ),
    }
    Ok(())
}

/// The worktree's change fingerprint against the base the review gate uses.
fn current_fingerprint(work_dir: &Path, stage: &Stage) -> Result<ChangeFingerprint> {
    let (worktree, target) = stage_worktree_and_target(work_dir, stage)?;
    fingerprint::compute(&worktree, &target)
}

/// The stage's worktree and the target branch the completion gates diff it
/// against, resolved from the CWD's repository root as `loom stage complete`
/// resolves it, so the `loom stage review` commands and completion agree.
pub(super) fn stage_worktree_and_target(
    work_dir: &Path,
    stage: &Stage,
) -> Result<(PathBuf, String)> {
    let worktree = resolve_stage_execution_paths(stage)?
        .worktree_root
        .with_context(|| format!("stage '{}' has no worktree", stage.id))?;
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let repo_root = find_repo_root_from_cwd(&cwd).unwrap_or(cwd);
    let target = crate::fs::resolve_target_branch_from_config(work_dir, &repo_root)?;
    Ok((worktree, target))
}

fn print_rounds(stage_id: &str, rounds: &[ReviewRound]) {
    if rounds.is_empty() {
        println!("No review round is recorded for stage '{stage_id}'.");
        return;
    }
    println!("Review rounds of stage '{stage_id}':");
    for round in rounds {
        let malformed = round
            .malformed
            .as_deref()
            .map(|reason| format!("  malformed: {}", single_line(reason)))
            .unwrap_or_default();
        println!(
            "  round {}  {}  findings={}{malformed}",
            round.round,
            round.fingerprint,
            round.findings.len()
        );
    }
}

fn print_open_findings(open: &[OpenFinding]) {
    if open.is_empty() {
        println!("Open findings: none");
        return;
    }
    println!("Open findings:");
    for item in open {
        let finding = &item.finding;
        let carried = item
            .origin_stage
            .as_deref()
            .map(|origin| format!("  (carried from {origin})"))
            .unwrap_or_default();
        println!(
            "  {}  {}  {}:{}  {}{carried}",
            item.id,
            single_line(&finding.severity),
            single_line(&finding.file),
            finding.line,
            single_line(&finding.claim)
        );
        for (label, value) in [("scenario", &finding.scenario), ("rule", &finding.rule)] {
            if let Some(value) = value.as_deref().filter(|value| !value.trim().is_empty()) {
                println!("      {label}: {}", single_line(value));
            }
        }
    }
}

/// Whether the latest round saw the current changes, and which files differ.
fn print_freshness(latest: Option<&ReviewRound>, current: &ChangeFingerprint) {
    println!("Current fingerprint: {}", current.value);
    let previous = match latest {
        Some(round) => {
            let matches = if round.fingerprint == current.value {
                "yes"
            } else {
                "no"
            };
            println!(
                "Latest round ({}) matches the current fingerprint: {matches}",
                round.round
            );
            if round.malformed.is_some() {
                println!(
                    "The latest round is malformed; the gate needs a well-formed round at the \
                     current fingerprint."
                );
            }
            round.files.clone()
        }
        None => {
            println!("Latest round: none");
            BTreeMap::new()
        }
    };
    println!("changed since last round:");
    let changed = fingerprint::changed_since(&previous, &current.files);
    if changed.is_empty() {
        println!("  (none)");
    }
    for path in changed {
        println!("  {}", single_line(&path));
    }
}
