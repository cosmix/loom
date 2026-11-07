//! Outcome formatting shared by the contract run and its format gate.

use std::collections::BTreeSet;
use std::os::fd::AsRawFd;
use std::path::{Component, Path};

use anyhow::{Context, Result};

use super::{child, CriterionOutcome};
use crate::daemon::ContractRunReport;
use crate::fs::safe_fs::{safe_open_dirfd, safe_remove_in_workdir};
use crate::git::worktree::WorktreeGit;
use crate::host_verify::digest;
use crate::models::dispute::truncate_to_byte_limit;
use crate::models::stage::CommandConfinement;
use crate::verify::contracts::freeze::{check_formatting, refused_problems};
use crate::verify::contracts::refusal::run_output_line;
use crate::verify::contracts::site::ContractSite;
use artefacts::GateWrites;

#[path = "freeze_artefacts.rs"]
mod artefacts;

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
/// afterwards, also when it refused, but only those [`GateWrites`] covers.
pub(super) fn format_outcome(
    site: &ContractSite,
    git: &WorktreeGit,
    confinement: CommandConfinement,
    written: &[String],
    contracts_compile: bool,
) -> Result<CriterionOutcome> {
    let before = digest::untracked_in(git)?;
    let writes = GateWrites::of(site, contracts_compile);
    let gate = check_formatting(site, confinement, written, contracts_compile);
    let removed = remove_gate_writes(git, &site.worktree_root, &before, &writes)?;
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

/// Remove the untracked files `git` lists now that were not in `before` and
/// that `writes` covers, below `root`; a path with a `..` or an absolute one
/// is skipped. Every other new file stays: a writer still running may have
/// added it, and the tree binding must refuse that change. Empty directories
/// stay too: git and the tree digest ignore them. Returns the paths removed.
fn remove_gate_writes(
    git: &WorktreeGit,
    root: &Path,
    before: &BTreeSet<String>,
    writes: &GateWrites,
) -> Result<Vec<String>> {
    let created: Vec<String> = digest::untracked_in(git)?
        .difference(before)
        .filter(|path| {
            Path::new(path)
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
                && writes.covers(path)
        })
        .cloned()
        .collect();
    remove_listed(root, &created)?;
    Ok(created)
}

/// Remove each of `paths`, relative to `root`, through a handle on `root`
/// that refuses a symlink at every component: the host job runs unconfined,
/// and a directory swapped for a link while the loop runs must not move a
/// removal outside the worktree. A file already gone is not an error.
fn remove_listed(root: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let root = root
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", root.display()))?;
    let dirfd = safe_open_dirfd(&root)?;
    for path in paths {
        safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new(path))
            .with_context(|| format!("cannot remove {path}, a file the gate created"))?;
    }
    Ok(())
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
