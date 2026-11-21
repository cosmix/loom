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
//! every error path and while unwinding. [`BaseCheckout::kept`] instead keeps
//! a checkout at a loom-owned directory beside the stage cache, which a
//! confined run at the base writes, so loom's own git never reads that
//! checkout's `.git` (`stage_cache::base_checkout`).
//!
//! A checkout holds tracked files only, so an installed environment
//! (`node_modules`, `.venv`) is missing from it. The plan's `provision`
//! entries (the snapshot `loom init` took) therefore run in the checkout
//! before any selection does, confined, with a build cache of the
//! comparison's own beside the checkout and removed with it (`provision`);
//! the pass cache is never used. Only then are the selections compared. A
//! provision that fails, or a selected runner still missing its environment
//! or refused by the sandbox, leaves a note `base not runnable: <reason>`
//! and no failure: the operator's decision (plan Decisions item 6) is that a
//! base which cannot run says nothing about the stage, and integration-verify's
//! full suite is the backstop. An unknown or unreadable base, a real build
//! failure, unreadable output, no selected test and a timeout still stand as
//! failures with what the base showed, and the stage disputes them (`loom
//! stage dispute-criteria --field impact-tests`). The compiled languages
//! still build from nothing within the selection timeout.
//!
//! The explicit `--at-base` job provisions its kept checkout the same way
//! (`host_verify::child_base`).

use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::failure::ImpactFailure;
use super::runs::{Failed, Run, Site};
use super::{selection_digest, FULL_SUITE};
use crate::git::changes::read_only;
use crate::git::runner::run_git_checked;
use crate::plan::schema::ProvisionEntry;
use crate::utils::text::single_line;
use crate::verify::criteria::ProbeRunner;
use crate::verify::review::fingerprint::local_git;

mod prepare;

pub(crate) use prepare::provision_base;
pub(super) use prepare::BaseCriteria;

/// Name prefix of a dry-run checkout's scratch directory, which the dry run's
/// sweep of earlier runs' leftovers matches on.
pub(crate) const DRY_RUN_PREFIX: &str = "loom-dry-run-";

/// Hex digits of the base commit a note shows.
const SHORT_COMMIT: usize = 12;

/// Makes the runner for selections in a base checkout, and provisions it.
pub(super) trait BaseRunner {
    /// The runner for the checkout whose top directory is `root`.
    fn runner_for(&self, root: &Path) -> Box<dyn ProbeRunner + '_>;

    /// Run the plan's provision entries in the checkout at `root`, before any
    /// selection runs there; `Err` is why the base cannot be provisioned.
    fn provision(&self, root: &Path) -> Result<(), String>;
}

/// The stage's base, for re-running the selections that failed.
pub(super) struct BaseProbe<'a> {
    /// The stage worktree, whose repository holds the base commit.
    pub(super) worktree: &'a Path,
    /// The stage's merge base with its target branch, or why it is unknown.
    pub(super) commit: Result<String, String>,
    pub(super) runner: &'a dyn BaseRunner,
}

/// A detached checkout of one commit in a scratch directory, removed on drop,
/// or kept at a directory of the caller's ([`Self::kept`]).
pub(crate) struct BaseCheckout {
    dir: Option<TempDir>,
    root: PathBuf,
    /// A kept checkout found clean at the commit, with what earlier runs built.
    reused: bool,
}

impl BaseCheckout {
    /// Check out `commit` of the repository holding `worktree`.
    pub(crate) fn create(worktree: &Path, commit: &str) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("loom-impact-base-")
            .tempdir()
            .context("failed to create a scratch directory for the base checkout")?;
        Self::check_out(dir, worktree, commit)
    }

    /// Like [`Self::create`], with the scratch directory made inside `parent`
    /// instead of the system temporary directory: a caller whose checkout
    /// builds gigabytes of artifacts keeps them on a disk that can hold them.
    pub(crate) fn create_in(parent: &Path, worktree: &Path, commit: &str) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix(DRY_RUN_PREFIX)
            .tempdir_in(parent)
            .with_context(|| format!("failed to create a checkout under {}", parent.display()))?;
        Self::check_out(dir, worktree, commit)
    }

    /// The shared clone and detached checkout both constructors make in `dir`.
    fn check_out(dir: TempDir, worktree: &Path, commit: &str) -> Result<Self> {
        let common = common_dir(worktree)?;
        let root = dir.path().join("checkout");
        let target = root
            .to_str()
            .context("the scratch directory is not UTF-8")?;
        let clone = ["clone", "--quiet", "--shared", "--no-checkout", "--"];
        let args: Vec<&str> = clone.into_iter().chain([common.as_str(), target]).collect();
        run_git_checked(&args, dir.path())?;
        run_git_checked(&["checkout", "--quiet", "--detach", commit], &root)?;
        Ok(Self {
            dir: Some(dir),
            root,
            reused: false,
        })
    }

    /// The checkout of `commit` of the repository holding `worktree` at `dir`,
    /// kept when dropped: reused when its HEAD is `commit` and its tree is
    /// clean, else deleted and made again. A symbolic link at `dir` or its
    /// parent is refused (`stage_cache::base_checkout::keep`).
    pub(crate) fn kept(dir: &Path, worktree: &Path, commit: &str) -> Result<Self> {
        let common = common_dir(worktree)?;
        let reused = crate::stage_cache::base_checkout::keep(dir, &common, commit)?;
        Ok(Self {
            dir: None,
            root: dir.to_path_buf(),
            reused,
        })
    }

    /// The checkout's top directory.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Whether [`Self::kept`] reused a checkout an earlier run left.
    pub(crate) fn reused(&self) -> bool {
        self.reused
    }
}

/// The absolute common git directory of the repository holding `worktree`,
/// read through git pinned to that checkout.
fn common_dir(worktree: &Path) -> Result<String> {
    let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    let common = read_only(&local_git(worktree)?, &args)?;
    let common = String::from_utf8(common).context("the git common directory is not UTF-8")?;
    Ok(common.trim().to_string())
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
                "failed to remove a temporary checkout"
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
    if let Err(reason) = base.runner.provision(checkout.root()) {
        notes.insert(not_runnable(&reason));
        return Vec::new();
    }
    let runner = base.runner.runner_for(checkout.root());
    let site = Site {
        root: checkout.root(),
        runner: runner.as_ref(),
        place: "the base checkout (tracked files, then the plan's provision)",
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

/// Run each of `entries` in order in `root` through `run`, stopping at the
/// first `Err`, which is the reason the base cannot be provisioned.
pub(crate) fn provision_with<F>(
    entries: &[ProvisionEntry],
    root: &Path,
    mut run: F,
) -> Result<(), String>
where
    F: FnMut(&ProvisionEntry, &Path) -> Result<(), String>,
{
    entries.iter().try_for_each(|entry| run(entry, root))
}

/// The note for a base that cannot run a selection for reasons of its own.
fn not_runnable(reason: &str) -> String {
    format!("base not runnable: {}; {FULL_SUITE}", single_line(reason))
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
    match site.run_selection(&failed.package, failed.adapter, &failed.targets) {
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
        Run::Inconclusive(why) if why.is_environment() => Ok(not_runnable(&why.to_string())),
        Run::Inconclusive(why) => Err(format!("the stage's base {short} proved nothing: {why}")),
    }
}

/// `failed` as the gate reports it, with what the base showed.
fn stands(failed: Failed, base: String) -> ImpactFailure {
    ImpactFailure {
        selection_digest: selection_digest(&failed.targets),
        tests: failed.tests(),
        id: failed.id,
        command: failed.failure.command,
        kind: failed.failure.kind,
        failed_tests: failed.failure.failed_tests,
        base,
        accepted: failed.accepted,
        output: failed.failure.output,
    }
}

/// Never makes a base runner: a test whose selections all pass, or whose base
/// is unknown, never needs one.
#[cfg(test)]
pub(crate) struct NoBaseRunner;

#[cfg(test)]
impl BaseRunner for NoBaseRunner {
    fn runner_for(&self, _root: &Path) -> Box<dyn ProbeRunner + '_> {
        unreachable!("no selection is re-run at the base")
    }

    fn provision(&self, _root: &Path) -> Result<(), String> {
        unreachable!("no selection is re-run at the base")
    }
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;
