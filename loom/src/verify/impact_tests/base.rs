//! Re-running failed selections at the stage's base (DESIGN D14). A
//! selection whose tests fail there too, with at least as many failures
//! counted at both, is a pre-existing failure: a note, never the stage's to
//! fix (`FailureKind::covers`). A selection that does not build, or whose
//! output or failure count could not be read, stays a failure with what the
//! base showed, even when the base ended the same way: the base checkout
//! often cannot build for reasons of its own (below), which would hide a
//! compile error the stage made. Such a failure goes to a dispute. Only the
//! selections that failed run again, each under the selection timeout.
//!
//! The base is checked out into a scratch directory of its own: a shared
//! clone (`git clone --shared --no-checkout`) of the repository's common git
//! directory, detached at the base commit. It borrows the repository's
//! objects and writes nothing outside the scratch directory, so a sandboxed
//! session can make one, and it has a `.git` of its own for a test that asks
//! git where it runs. [`BaseCheckout`] removes the directory when dropped, on
//! every error path and while unwinding.
//!
//! A checkout holds tracked files only. A package whose installed
//! environment (`node_modules`, `.venv`) is untracked is not run there, and a
//! compiled language builds from nothing within the selection timeout; either
//! way the failure stands with the reason and the stage disputes it (`loom
//! stage dispute-criteria --field impact-tests`). Revisit when such disputes
//! become routine: the base run could borrow the worktree's environment and
//! build cache.

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::failure::ImpactFailure;
use super::runs::{Failed, Run, Site};
use super::FULL_SUITE;
use crate::git::runner::run_git_checked;
use crate::models::stage::Stage;
use crate::verify::contracts::changes::git;
use crate::verify::criteria::{CriteriaConfig, CriteriaProbe, ProbeRunner};
use crate::verify::review::fingerprint::local_git;
use crate::verify::review::report::single_line;

/// Hex digits of the base commit a note shows.
const SHORT_COMMIT: usize = 12;

/// Makes the runner for selections in a base checkout.
pub(super) trait BaseRunner {
    /// The runner for the checkout whose top directory is `root`.
    fn runner_for(&self, root: &Path) -> Box<dyn ProbeRunner + '_>;
}

/// The acceptance-criteria runner a base checkout gets: the selection's own
/// configuration with the checkout as its tree, and no pass cache, since a
/// scratch copy's runs are worth nothing once it is gone.
pub(super) struct BaseCriteria<'a> {
    pub(super) stage: &'a Stage,
    pub(super) config: CriteriaConfig,
}

impl BaseRunner for BaseCriteria<'_> {
    fn runner_for(&self, root: &Path) -> Box<dyn ProbeRunner + '_> {
        let config = CriteriaConfig {
            cache_dir: None,
            ..self.config.clone()
        };
        Box::new(CriteriaProbe {
            stage: self.stage,
            config: config.with_tree_root(Some(root)),
        })
    }
}

/// The stage's base, for re-running the selections that failed.
pub(super) struct BaseProbe<'a> {
    /// The stage worktree, whose repository holds the base commit.
    pub(super) worktree: &'a Path,
    /// The stage's merge base with its target branch, or why it is unknown.
    pub(super) commit: Result<String, String>,
    pub(super) runner: &'a dyn BaseRunner,
}

/// A detached checkout of one commit in a scratch directory, removed on drop.
pub(super) struct BaseCheckout {
    dir: Option<TempDir>,
    root: PathBuf,
}

impl BaseCheckout {
    /// Check out `commit` of the repository holding `worktree`.
    pub(super) fn create(worktree: &Path, commit: &str) -> Result<Self> {
        let common_dir = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
        let common = git(&local_git(worktree)?, &common_dir)?;
        let common = String::from_utf8(common).context("the git common directory is not UTF-8")?;
        let dir = tempfile::Builder::new()
            .prefix("loom-impact-base-")
            .tempdir()
            .context("failed to create a scratch directory for the base checkout")?;
        let root = dir.path().join("checkout");
        let target = root
            .to_str()
            .context("the scratch directory is not UTF-8")?;
        let clone = ["clone", "--quiet", "--shared", "--no-checkout", "--"];
        let args: Vec<&str> = clone.into_iter().chain([common.trim(), target]).collect();
        run_git_checked(&args, dir.path())?;
        run_git_checked(&["checkout", "--quiet", "--detach", commit], &root)?;
        Ok(Self {
            dir: Some(dir),
            root,
        })
    }

    /// The checkout's top directory.
    pub(super) fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for BaseCheckout {
    fn drop(&mut self) {
        let Some(dir) = self.dir.take() else {
            return;
        };
        let path = dir.path().to_path_buf();
        if let Err(error) = dir.close() {
            tracing::warn!(
                path = %path.display(),
                %error,
                "failed to remove the impact-test base checkout"
            );
        }
    }
}

/// Re-run each of `failed` at the base. One failing there no better becomes
/// a note in `notes`; every other stays a failure saying what the base showed.
pub(super) fn compare(
    base: &BaseProbe<'_>,
    failed: Vec<Failed>,
    notes: &mut BTreeSet<String>,
) -> Vec<ImpactFailure> {
    if failed.is_empty() {
        return Vec::new();
    }
    let commit = match &base.commit {
        Ok(commit) => commit,
        Err(why) => {
            let why = format!("the stage's base is unknown ({why}), so it was not re-run there");
            return failed.into_iter().map(|f| stands(f, why.clone())).collect();
        }
    };
    let short = &commit[..commit.len().min(SHORT_COMMIT)];
    let checkout = match BaseCheckout::create(base.worktree, commit) {
        Ok(checkout) => checkout,
        Err(error) => {
            let why = format!(
                "the stage's base {short} could not be checked out ({}), so it was not re-run \
                 there",
                single_line(&format!("{error:#}"))
            );
            return failed.into_iter().map(|f| stands(f, why.clone())).collect();
        }
    };
    let runner = base.runner.runner_for(checkout.root());
    let site = Site {
        root: checkout.root(),
        runner: runner.as_ref(),
        place: "the base checkout (it holds tracked files only)",
    };
    let mut standing = Vec::new();
    for failed in failed {
        match at_base(&site, &failed, short) {
            Ok(note) => {
                notes.insert(note);
            }
            Err(base) => standing.push(stands(failed, base)),
        }
    }
    standing
}

/// The pre-existing note when `failed` fails at the base no better, else
/// what the base run showed.
///
/// The comparison is by count only: a base run counting at least as many
/// failed tests proves the failure pre-existing without naming which tests
/// failed. The base checkout holds tracked files only, so an untracked
/// fixture or generated file the stage's tests need is missing there and can
/// inflate the base count; a stage failure that its diff caused then passes
/// as pre-existing. Revisit if an adapter learns to name failed tests.
fn at_base(site: &Site<'_>, failed: &Failed, short: &str) -> Result<String, String> {
    let kind = &failed.failure.kind;
    match site.run(&failed.package, failed.adapter, &failed.targets) {
        Run::Failed(base) if base.kind.covers(kind) => Ok(format!(
            "`{}` {} at the stage's base {short} as well ({}): a failure this stage did not \
             cause; {FULL_SUITE}",
            failed.id,
            kind.describe(),
            base.kind.describe()
        )),
        Run::Failed(base) if base.kind == *kind => Err(format!(
            "it {} at the stage's base {short} as well, which proves no failure pre-existing: \
             only failed tests counted at both are",
            base.kind.describe()
        )),
        Run::Failed(base) => Err(format!(
            "at the stage's base {short} it {} instead",
            base.kind.describe()
        )),
        Run::Passed { .. } => Err(format!("it passes at the stage's base {short}")),
        Run::Inconclusive(why) => Err(format!("the stage's base {short} proved nothing: {why}")),
    }
}

/// `failed` as the gate reports it, with what the base showed.
fn stands(failed: Failed, base: String) -> ImpactFailure {
    ImpactFailure {
        tests: failed.tests(),
        id: failed.id,
        command: failed.failure.command,
        kind: failed.failure.kind,
        base,
        accepted: failed.accepted,
        output: failed.failure.output,
    }
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;
