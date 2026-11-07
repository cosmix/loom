//! Outcome formatting shared by the contract run and its format gate.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use anyhow::{Context, Result};

use super::{child, CriterionOutcome};
use crate::daemon::ContractRunReport;
use crate::git::worktree::WorktreeGit;
use crate::host_verify::digest;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::CommandConfinement;
use crate::verify::contracts::freeze::{check_formatting, refused_problems};
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::contracts::site::ContractSite;

/// The summary of a contract the run classified red; `shared_from` names the
/// contract whose failed build it took instead of running its own.
pub(super) fn verdict_line(report: &ContractRunReport, shared_from: Option<&str>) -> String {
    let id = &report.contract_id;
    let exit = report
        .exit_code
        .map_or_else(|| "no exit code".to_string(), |code| format!("exit {code}"));
    let shared = shared_from.map_or_else(String::new, |first| {
        format!("; shares the failed build of contract `{first}`, not run again")
    });
    format!(
        "contract `{id}` is red ({}, {exit}), as the freeze requires{shared}",
        report.outcome
    )
}

/// The static gate as outcome `format`. A gate that could not run is an
/// error of the run, never a failed outcome.
///
/// The gate's own checks may write files (a lint cache such as
/// `tsconfig.tsbuildinfo`); an untracked file that is not git-ignored moves
/// the tree digest, and the freeze job compares the digest before and after
/// the run. The untracked files the gate created are therefore removed
/// afterwards, also when it refused.
pub(super) fn format_outcome(
    site: &ContractSite,
    git: &WorktreeGit,
    confinement: CommandConfinement,
    written: &[String],
    contracts_compile: bool,
) -> Result<CriterionOutcome> {
    let before = digest::untracked_in(git)?;
    let gate = check_formatting(site, confinement, written, contracts_compile);
    let removed = remove_created(git, &site.worktree_root, &before)?;
    let (passed, mut summary) = match gate {
        Ok(()) => (
            true,
            "every contract and harness file passes the stage's formatter, lint and search checks"
                .to_string(),
        ),
        Err(error) => match refused_problems(&error) {
            Some(problems) => (false, problems.join("\n")),
            None => return Err(error),
        },
    };
    if !removed.is_empty() {
        // File names come from the worktree, so they are quoted as run output.
        let note = format!("removed files the checks created: {}", removed.join(", "));
        summary = format!("{summary}\n{}", run_output_line(&note));
    }
    Ok(outcome("format".to_string(), passed, &summary))
}

/// Remove the untracked files `git` lists now that were not in `before`,
/// below `root`. Git reports a path inside the worktree and `remove_file`
/// does not follow a final symlink; a path with a `..` or an absolute one is
/// skipped. A file already gone is not an error. Empty directories stay: git
/// and the tree digest ignore them. Returns the paths removed.
///
/// This assumes the writer is waiting on the freeze: it writes nothing into
/// the worktree meanwhile, so every file new since `before` came from the gate
/// and none is a concurrent writer file.
fn remove_created(
    git: &WorktreeGit,
    root: &Path,
    before: &BTreeSet<String>,
) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    for path in digest::untracked_in(git)?.difference(before) {
        let plain = Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
        if !plain {
            continue;
        }
        match std::fs::remove_file(root.join(path)) {
            Ok(()) => removed.push(path.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot remove {path}, a file the gate created"));
            }
        }
    }
    Ok(removed)
}

pub(super) fn outcome(label: String, passed: bool, summary: &str) -> CriterionOutcome {
    CriterionOutcome {
        label,
        passed,
        summary: truncate_to_byte_limit(summary, child::SUMMARY_MAX_BYTES),
        log: None,
    }
}

#[cfg(test)]
#[path = "freeze_format_tests.rs"]
mod tests;
