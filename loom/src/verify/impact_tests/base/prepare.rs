//! Provisioning a base checkout and the runner that runs selections in it.
//!
//! A scratch comparison gets a build cache of its own, a fresh
//! [`StageCache`] rooted in the comparison's scratch directory beside the
//! checkout and removed with it. The stage's worktree cache is mutable by the
//! stage's own code and is never read here, and the explicit at-base job's
//! kept directories are neither reused nor pruned (`stage_cache::base_checkout`).

use std::path::Path;
use std::slice;

use anyhow::{Context, Result};

use super::{provision_with, BaseRunner};
use crate::models::stage::{CommandConfinement, Stage};
use crate::orchestrator::provision::{read_provision_snapshot, run_provision_in_stage};
use crate::plan::schema::ProvisionEntry;
use crate::stage_cache::StageCache;
use crate::verify::criteria::{resolve_confinement, CriteriaConfig, CriteriaProbe, ProbeRunner};

/// Directory of the comparison's build cache, beside its checkout.
const CACHE_DIR: &str = "cache";

/// [`provision_with`] through the confined provision runner, filling `cache`,
/// the build cache a selection or criterion run at this base reads.
pub(crate) fn provision_base(
    entries: &[ProvisionEntry],
    root: &Path,
    cache: &StageCache,
) -> Result<(), String> {
    provision_with(entries, root, |entry, dir| {
        run_provision_in_stage(slice::from_ref(entry), dir, Some(cache))
    })
}

/// The build cache of the scratch comparison whose checkout is at `root`: in
/// `root`'s parent, the scratch directory `BaseCheckout::create` made, so it is
/// removed with the checkout. Its name is keyed by the checkout, so no other
/// comparison and no stage names it.
fn base_cache(root: &Path, stage_id: &str) -> Result<StageCache, String> {
    let scratch = root.parent().ok_or_else(|| {
        format!(
            "the base checkout {} has no scratch directory for a build cache",
            root.display()
        )
    })?;
    StageCache::resolve(root, stage_id, scratch, Some(&scratch.join(CACHE_DIR)))
        .map_err(|error| format!("no build cache for the base checkout: {error:#}"))
}

/// The acceptance-criteria runner a base checkout gets: the selection's own
/// configuration with the checkout as its tree, no pass cache (a scratch
/// copy's runs are worth nothing once it is gone) and the comparison's own
/// build cache.
pub(in crate::verify::impact_tests) struct BaseCriteria<'a> {
    stage: &'a Stage,
    config: CriteriaConfig,
    entries: Vec<ProvisionEntry>,
}

impl<'a> BaseCriteria<'a> {
    /// The runner for `stage`, provisioning with the entries the state
    /// directory `work_dir` snapshots. A snapshot that cannot be read is a
    /// configuration error, never a skipped provision.
    pub(in crate::verify::impact_tests) fn new(
        stage: &'a Stage,
        config: CriteriaConfig,
        work_dir: &Path,
    ) -> Result<Self> {
        let entries = read_provision_snapshot(work_dir)
            .context("the plan's provision snapshot in the state directory is unreadable")?;
        Ok(Self {
            stage,
            config,
            entries,
        })
    }

    /// Whether the stage's commands run confined, the only runs a build cache
    /// is granted to.
    fn confined(&self) -> bool {
        let level = resolve_confinement(
            self.stage.sandbox.command_confinement,
            self.config.plan_confinement,
        );
        level == CommandConfinement::Confined
    }
}

impl BaseCriteria<'_> {
    /// The configuration a run at the checkout `root` gets: no pass cache and,
    /// for confined commands, the build cache `provision` filled.
    fn probe_config(&self, root: &Path) -> CriteriaConfig {
        // `provision` already refused a base whose cache cannot be made, so a
        // missing cache here only leaves each command's own private one.
        let stage_cache = base_cache(root, &self.stage.id)
            .ok()
            .filter(|_| self.confined());
        let config = CriteriaConfig {
            cache_dir: None,
            stage_cache,
            ..self.config.clone()
        };
        config.with_tree_root(Some(root))
    }

    /// [`BaseRunner::provision`] with `run` filling the build cache it is
    /// handed, the one [`Self::probe_config`] gives the runs.
    fn provision_through(
        &self,
        root: &Path,
        run: impl FnOnce(&[ProvisionEntry], &Path, &StageCache) -> Result<(), String>,
    ) -> Result<(), String> {
        run(&self.entries, root, &base_cache(root, &self.stage.id)?)
    }
}

impl BaseRunner for BaseCriteria<'_> {
    fn runner_for(&self, root: &Path) -> Box<dyn ProbeRunner + '_> {
        Box::new(CriteriaProbe {
            stage: self.stage,
            config: self.probe_config(root),
        })
    }

    fn provision(&self, root: &Path) -> Result<(), String> {
        self.provision_through(root, provision_base)
    }
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
