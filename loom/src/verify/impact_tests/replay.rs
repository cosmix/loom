//! Reconstructing the command of a disputed selection, read-only, from the
//! checkout itself. A failure record carries a display excerpt of its command
//! and bounded test names, all of it the completion run's own claim, so the
//! adjudicator is given a command loom derives again from the checkout's
//! graph. It is shown only when the selection it names has the digest the
//! failure reported (`selection_digest`); otherwise the checkout selects
//! something else and the evidence is stale.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

use super::failure::{package_label, selection_digest, split_failure_id, ImpactFailure};
use super::{checkout_git, plan, scoped, Plan};
use crate::context::worktree_graph::build_for_worktree;
use crate::git::changes::changes_since;
use crate::models::stage::Stage;

/// A command to run a selection again, and where to run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replay {
    /// The package directory, in the checkout the plan was derived from.
    pub(crate) directory: PathBuf,
    /// The complete selection command.
    pub(crate) command: String,
}

/// Every selection the stage's changes select in a checkout, to look the
/// selection of a failure up in.
pub(crate) struct ReplayPlan {
    plan: Plan,
}

impl ReplayPlan {
    /// The selections of the stage's changes since `base` in the checkout
    /// holding `working_dir`, derived as a completion run derives them. Fails
    /// when the graph, the changes or the contract files cannot be read.
    pub(crate) fn build(stage: &Stage, working_dir: &Path, base: &str) -> Result<Self> {
        let graph =
            build_for_worktree(working_dir).context("failed to build the worktree source graph")?;
        let git = checkout_git(working_dir).context("failed to find the worktree root")?;
        let changed: Vec<PathBuf> = changes_since(&git, base, None)
            .with_context(|| format!("failed to list the changes since {base}"))?
            .into_iter()
            .map(PathBuf::from)
            .collect();
        Ok(Self {
            plan: plan(stage, working_dir, &graph, &changed)?,
        })
    }

    /// The command of `failure`'s selection, when the checkout selects exactly
    /// the tests the failure reported. A failure of another runner or package,
    /// one without a digest, and one whose selection the checkout derives
    /// otherwise are each an error saying why.
    pub(crate) fn replay(&self, failure: &ImpactFailure) -> Result<Replay> {
        let id = &failure.id;
        let Some((runner, label)) = split_failure_id(id) else {
            bail!("'{id}' is not an impact-test id");
        };
        if failure.selection_digest.is_empty() {
            bail!("the evidence for {id} carries no selection digest");
        }
        let Some(((package, _), (adapter, targets))) = self
            .plan
            .groups
            .iter()
            .find(|((package, name), _)| *name == runner && package_label(package) == label)
        else {
            bail!("the checkout selects no {runner} tests in `{label}`");
        };
        if selection_digest(targets) != failure.selection_digest {
            bail!("the checkout selects other tests for {id} than the evidence reported");
        }
        let command = scoped::select_command(&self.plan.root, package, *adapter, targets)
            .with_context(|| format!("{runner} cannot select the tests of {id}"))?;
        let root = &self.plan.root;
        let directory = if package.as_os_str().is_empty() {
            root.clone()
        } else {
            root.join(package)
        };
        Ok(Replay { directory, command })
    }

    /// A plan of one selection, for a test of a caller.
    #[cfg(test)]
    pub(crate) fn of_selection(
        root: &Path,
        runner: &str,
        package: &Path,
        targets: Vec<crate::testrun::TestTarget>,
    ) -> Self {
        let adapter = crate::testrun::registry::by_name(runner).expect("a registered runner");
        let mut groups = super::Groups::new();
        groups.insert((package.to_path_buf(), adapter.name()), (adapter, targets));
        Self {
            plan: Plan {
                root: root.to_path_buf(),
                groups,
                notes: Default::default(),
            },
        }
    }
}

#[cfg(test)]
#[path = "replay_tests.rs"]
mod tests;
