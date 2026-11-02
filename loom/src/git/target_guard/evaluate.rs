//! Evaluating a move of the target that loom did not make, from the accepted
//! tip to the observed one, using only the record, the ledger and git
//! commands run through the runner.

use anyhow::{bail, Result};
use std::collections::HashSet;
use std::path::Path;

use super::attestation::{ledger_steps, LedgerStep};
use super::record::knowledge_prefix;
use super::HoldReason;
use crate::git::branch::{branch_ref, is_ancestor_of};
use crate::git::merge::control_paths::{changed_paths, hooks_dir_prefix_checked, is_control_path};
use crate::git::runner::{run_git, run_git_checked};

/// Most paths an `Unattested` reason names.
const MAX_GAP_PATHS: usize = 20;

/// Where a move is judged: the repository, the state directory and the
/// target's record key.
pub(super) struct Scope<'a> {
    pub(super) repo_root: &'a Path,
    pub(super) work_dir: &'a Path,
    pub(super) key: &'a str,
}

/// `error` with its causes on one line.
pub(super) fn one_line(error: &anyhow::Error) -> String {
    format!("{error:#}")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

/// The reasons to hold the move from `accepted` to `tip`; none accept it.
/// `required` walks the attestation ledger. A git error is the single reason
/// `Unevaluable`.
pub(super) fn evaluate(
    scope: &Scope,
    accepted: &str,
    tip: &str,
    required: bool,
) -> Vec<HoldReason> {
    reasons(scope, accepted, tip, required).unwrap_or_else(|error| {
        vec![HoldReason::Unevaluable {
            error: one_line(&error),
        }]
    })
}

fn reasons(scope: &Scope, accepted: &str, tip: &str, required: bool) -> Result<Vec<HoldReason>> {
    let repo = scope.repo_root;
    let mut reasons = Vec::new();
    // `is_ancestor_of` errors on a git failure; `run_git_bool` would hide it.
    let fast_forward = is_ancestor_of(accepted, tip, repo)?;
    if !fast_forward {
        reasons.push(HoldReason::NotFastForward);
    }
    let hooks_prefix = hooks_dir_prefix_checked(repo)?;
    let paths: Vec<String> = changed_paths(repo, accepted, tip)?
        .into_iter()
        .filter(|path| is_control_path(path, hooks_prefix.as_deref()))
        .collect();
    if !paths.is_empty() {
        reasons.push(HoldReason::ControlPaths { paths });
    }
    if !fast_forward {
        return Ok(reasons);
    }
    if required {
        reasons.extend(attestation_gap(scope, accepted, tip)?);
    }
    let branches = stage_work(repo, accepted, tip)?;
    if !branches.is_empty() {
        reasons.push(HoldReason::StageWork { branches });
    }
    Ok(reasons)
}

/// The first unattested gap of the ledger's chain from `accepted` to `tip`
/// that changes paths outside the knowledge prefix, if any.
///
/// Two walks of the chain run, and either one reaching `tip` attests the
/// move. The first follows forward steps only. The second may also follow
/// steps that lead backwards (a reset, a commit, then a merge of the old tip
/// is attested history). When both stop at a gap, the second walk's gap is
/// the reason if it starts at `accepted` or a descendant of it, else the
/// first walk's: a reset and its undo leave a backward and a forward step
/// between the same commits, and the second walk takes the backward one, so
/// its gap starts behind `accepted` and charges accepted commits to it.
fn attestation_gap(scope: &Scope, accepted: &str, tip: &str) -> Result<Option<HoldReason>> {
    let repo = scope.repo_root;
    let steps = ledger_steps(scope.work_dir, &branch_ref(scope.key))?;
    let Some(forward) = chain_gap(repo, &steps, accepted, tip, true)? else {
        return Ok(None);
    };
    let Some(unrestricted) = chain_gap(repo, &steps, accepted, tip, false)? else {
        return Ok(None);
    };
    let ahead = matches!(
        &unrestricted,
        HoldReason::Unattested { from, .. } if reaches(repo, accepted, from)
    );
    Ok(Some(if ahead { unrestricted } else { forward }))
}

/// The first gap of one walk of the chain from `accepted` to `tip`, if any.
///
/// From each commit `at` the walk follows an attested step to an unvisited
/// commit that is `tip` or one of its ancestors (with `forward_only`, also a
/// descendant of `at`): the step to `tip` when there is one, else the latest
/// in ledger order. With no such step it crosses an unattested gap to the
/// earliest unvisited ledger `from` between `at` and `tip`, or to `tip`.
/// Knowledge stages commit to the target from a sandbox the hook cannot
/// attest, so a gap touching only `doc/loom/knowledge/` passes. Every
/// iteration that does not return visits a new `from`, `to` or `tip`, which
/// bounds the walk and stops a ledger of resets from cycling.
fn chain_gap(
    repo: &Path,
    steps: &[LedgerStep],
    accepted: &str,
    tip: &str,
    forward_only: bool,
) -> Result<Option<HoldReason>> {
    let mut at = accepted.to_string();
    let mut visited = HashSet::from([at.clone()]);
    for _ in 0..2 * steps.len() + 2 {
        if at == tip {
            return Ok(None);
        }
        let next = match attested_step(repo, steps, &at, tip, &visited, forward_only) {
            Some(to) => to,
            None => {
                let end = gap_end(repo, steps, &at, tip, &visited)?;
                let paths = unattested_paths(repo, &at, &end)?;
                if !paths.is_empty() {
                    return Ok(Some(HoldReason::Unattested {
                        from: at,
                        to: end,
                        paths,
                    }));
                }
                end
            }
        };
        visited.insert(next.clone());
        at = next;
    }
    Ok(Some(HoldReason::Unevaluable {
        error: "attestation chain did not converge".to_string(),
    }))
}

/// Whether `ancestor` is `descendant` or one of its ancestors. An operand
/// from the ledger may be a commit a reset abandoned and `gc` pruned: a git
/// failure counts as "no", never as an error.
fn reaches(repo: &Path, ancestor: &str, descendant: &str) -> bool {
    matches!(is_ancestor_of(ancestor, descendant, repo), Ok(true))
}

/// Where an attested step from `at` leads, if one reaches an unvisited
/// commit that is `tip` or one of its ancestors and, with `forward_only`, a
/// descendant of `at`. `at` is `tip` or an ancestor of it, so a step to `tip`
/// is always forward.
fn attested_step(
    repo: &Path,
    steps: &[LedgerStep],
    at: &str,
    tip: &str,
    visited: &HashSet<String>,
    forward_only: bool,
) -> Option<String> {
    let from_here = || {
        steps
            .iter()
            .filter(move |step| step.from == at && !visited.contains(&step.to))
    };
    if from_here().any(|step| step.to == tip) {
        return Some(tip.to_string());
    }
    from_here()
        .rev()
        .find(|step| reaches(repo, &step.to, tip) && (!forward_only || reaches(repo, at, &step.to)))
        .map(|step| step.to.clone())
}

/// Where an unattested gap from `at` ends: among the unvisited ledger `from`
/// values that descend from `at` and are `tip` or its ancestors, the one that
/// is an ancestor of all the others; `tip` when there is none.
fn gap_end(
    repo: &Path,
    steps: &[LedgerStep],
    at: &str,
    tip: &str,
    visited: &HashSet<String>,
) -> Result<String> {
    // One rev-list instead of two ancestry tests per ledger value; `at` is
    // always `tip` or an ancestor of it.
    let range = format!("{at}..{tip}");
    let between = run_git_checked(&["rev-list", "--ancestry-path", &range], repo)?;
    let between: HashSet<&str> = between.lines().collect();
    let mut candidates: Vec<&str> = Vec::new();
    for from in steps.iter().map(|step| step.from.as_str()) {
        if between.contains(from) && !visited.contains(from) && !candidates.contains(&from) {
            candidates.push(from);
        }
    }
    let earliest = candidates.iter().find(|candidate| {
        candidates
            .iter()
            .all(|other| other == *candidate || reaches(repo, candidate, other))
    });
    Ok(earliest.map_or_else(|| tip.to_string(), |candidate| candidate.to_string()))
}

/// The paths `from`..`to` changes outside the knowledge prefix, at most
/// [`MAX_GAP_PATHS`].
fn unattested_paths(repo: &Path, from: &str, to: &str) -> Result<Vec<String>> {
    let prefix = knowledge_prefix();
    Ok(changed_paths(repo, from, to)?
        .into_iter()
        .filter(|path| !path.starts_with(prefix))
        .take(MAX_GAP_PATHS)
        .collect())
}

/// The `loom/*` branches with commits in `tip` that `accepted` lacks, by
/// short name. Only refs to commits are judged: a ref written by hand can
/// name a blob, a tree or a tag, and `merge-base` failing on it would make
/// every evaluation fail.
fn stage_work(repo: &Path, accepted: &str, tip: &str) -> Result<Vec<String>> {
    let args = [
        "for-each-ref",
        "--format=%(objecttype) %(refname)",
        "refs/heads/loom/",
    ];
    let mut branches = Vec::new();
    let listing = run_git_checked(&args, repo)?;
    let commits = listing
        .lines()
        .filter_map(|line| line.strip_prefix("commit "));
    for reference in commits {
        if carries_unaccepted_work(repo, accepted, tip, reference)? {
            let short = reference.strip_prefix("refs/heads/").unwrap_or(reference);
            branches.push(short.to_string());
        }
    }
    Ok(branches)
}

/// Whether some merge base of `tip` and `branch` is not in `accepted`: then
/// `tip` carries commits of `branch` loom never accepted. Judged per merge
/// base, never by the branch tip, so a target moved onto an early commit of
/// the branch counts and a branch loom already merged does not. A branch
/// that shares no history with `tip` carries nothing.
fn carries_unaccepted_work(repo: &Path, accepted: &str, tip: &str, branch: &str) -> Result<bool> {
    let output = run_git(&["merge-base", "--all", tip, branch], repo)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    match output.status.code() {
        Some(0) => {}
        Some(1) if stdout.trim().is_empty() => return Ok(false),
        _ => bail!(
            "git merge-base --all {tip} {branch} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
    for base in stdout.lines().filter(|line| !line.is_empty()) {
        if !is_ancestor_of(base, accepted, repo)? {
            return Ok(true);
        }
    }
    Ok(false)
}
