//! Configuration types for acceptance criteria execution

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, ensure, Result};

use crate::models::stage::{CommandConfinement, Stage, TruthCheck};
use crate::stage_cache::StageCache;

use super::cache::CachePolicy;
use super::live_output::LiveOutput;
use super::memory_journals::MemoryJournals;
use super::result::CriterionResult;

/// Default timeout for command execution (5 minutes)
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// Default timeout for a `before_stage` or `after_stage` truth check that sets
/// no `timeout_secs` (30 seconds)
pub const DEFAULT_CHECK_TIMEOUT: Duration = Duration::from_secs(30);

/// Default timeout for an extended acceptance criterion that sets no
/// `timeout_secs` (5 minutes): such a criterion runs a build or a test suite
/// as often as a quick check.
pub const DEFAULT_EXTENDED_CRITERION_TIMEOUT: Duration = Duration::from_secs(300);

/// The timeout an extended acceptance criterion runs under: its own
/// `timeout_secs`, else [`DEFAULT_EXTENDED_CRITERION_TIMEOUT`].
pub fn extended_criterion_timeout(check: &TruthCheck) -> Duration {
    check.timeout(DEFAULT_EXTENDED_CRITERION_TIMEOUT)
}

/// Configuration for acceptance criteria execution
#[derive(Debug, Clone)]
pub struct CriteriaConfig {
    /// Maximum time to wait for a single command to complete
    pub command_timeout: Duration,

    /// Plan-level confinement default for the commands this run executes.
    ///
    /// The runner combines it with the stage's own override (see
    /// [`crate::verify::criteria::resolve_confinement`]); `None` means the plan
    /// said nothing, which leaves the effective level at `Confined`.
    pub plan_confinement: Option<CommandConfinement>,

    /// The plan's `sandbox.network.allow_local_binding`
    /// ([`crate::verify::criteria::plan_local_binding`]); a stage's own
    /// `network` section replaces it (`stage_network`). The
    /// stage's confined commands get this host's loopback when it is set.
    pub plan_local_binding: bool,

    /// Whether the runner may consult and update the on-disk acceptance pass
    /// cache (see `super::cache`). Read from `LOOM_ACCEPTANCE_CACHE` at
    /// construction time; a caller can still force it with
    /// [`Self::with_cache_policy`].
    pub cache: CachePolicy,

    /// Loom state directory (`.loom/work/`) the pass cache is stored under.
    /// `None` disables caching outright, regardless of `cache` — this is the
    /// default so a bare `CriteriaConfig::default()` (as used by tests and by
    /// [`super::run_acceptance`]) never touches disk for caching.
    pub cache_dir: Option<PathBuf>,

    /// The working tree a confined command may read and write: the stage's
    /// worktree root, which contains the directory the command starts in.
    /// `None` confines each command to its own working directory.
    pub tree_root: Option<PathBuf>,

    /// The build-cache root of the stage whose tree this run verifies: confined
    /// commands write it and run with its variables. `None` keeps each
    /// command's caches private and empty.
    pub stage_cache: Option<StageCache>,

    /// The plan's memory journals, outside `tree_root`, that confined commands
    /// may read, never write ([`super::memory_journals_grant`]). Their content is no
    /// input the pass cache fingerprints, so a run with them neither reads nor
    /// stores cached passes. `None` grants nothing beyond the tree.
    pub memory_journals: Option<MemoryJournals>,

    /// Told each criterion's 0-based index and result as it finishes, before
    /// the next one starts. `None` by default.
    pub observer: Option<CriterionObserver>,

    /// Where each criterion's output is copied while it runs. `None` by
    /// default; the captured result does not depend on it.
    pub live_output: Option<LiveOutput>,

    /// The 0-based indices of the criteria to run; `None` runs every one. A
    /// criterion outside the set is neither run, cached nor reported.
    pub only: Option<BTreeSet<usize>>,
}

/// The function a [`CriterionObserver`] wraps.
pub type CriterionCallback = dyn Fn(usize, &CriterionResult) + Send + Sync;

/// Callback the runner invokes as each acceptance criterion finishes.
#[derive(Clone)]
pub struct CriterionObserver(pub Arc<CriterionCallback>);

impl CriterionObserver {
    pub(super) fn notify(&self, index: usize, result: &CriterionResult) {
        (self.0)(index, result);
    }
}

impl fmt::Debug for CriterionObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CriterionObserver").finish_non_exhaustive()
    }
}

impl Default for CriteriaConfig {
    fn default() -> Self {
        Self {
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
            plan_confinement: None,
            plan_local_binding: false,
            cache: CachePolicy::from_env(),
            cache_dir: None,
            tree_root: None,
            stage_cache: None,
            memory_journals: None,
            observer: None,
            live_output: None,
            only: None,
        }
    }
}

impl CriteriaConfig {
    /// Create a new configuration with a custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            command_timeout: timeout,
            ..Self::default()
        }
    }

    /// Carry the plan-level confinement default into this run.
    pub fn with_plan_confinement(mut self, plan_confinement: Option<CommandConfinement>) -> Self {
        self.plan_confinement = plan_confinement;
        self
    }

    /// Carry the plan-level `allow_local_binding` default into this run.
    pub fn with_plan_local_binding(mut self, plan_local_binding: bool) -> Self {
        self.plan_local_binding = plan_local_binding;
        self
    }

    /// Enable the acceptance pass cache, stored under `work_dir`.
    pub fn with_cache_dir(mut self, work_dir: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(work_dir.into());
        self
    }

    /// Force the cache policy, overriding whatever `LOOM_ACCEPTANCE_CACHE`
    /// produced.
    pub fn with_cache_policy(mut self, policy: CachePolicy) -> Self {
        self.cache = policy;
        self
    }

    /// Let confined commands use all of `tree_root`, the working tree
    /// containing their working directory.
    pub fn with_tree_root(mut self, tree_root: Option<&Path>) -> Self {
        self.tree_root = tree_root.map(Path::to_path_buf);
        self
    }

    /// Give confined commands the stage's build caches.
    pub fn with_stage_cache(mut self, stage_cache: Option<StageCache>) -> Self {
        self.stage_cache = stage_cache;
        self
    }

    /// Let confined commands read `journals` (see [`Self::memory_journals`]).
    pub fn with_memory_journals(mut self, journals: Option<MemoryJournals>) -> Self {
        self.memory_journals = journals;
        self
    }

    /// Report each criterion to `observer` as it finishes.
    pub fn with_observer(mut self, observer: CriterionObserver) -> Self {
        self.observer = Some(observer);
        self
    }

    /// Copy each criterion's output to the writer `live` opens for it.
    pub fn with_live_output(mut self, live: LiveOutput) -> Self {
        self.live_output = Some(live);
        self
    }

    /// Run only the criteria at `only` (0-based); `None` runs every one.
    pub fn with_only(mut self, only: Option<BTreeSet<usize>>) -> Self {
        self.only = only;
        self
    }

    /// Whether this run includes the criterion at `index`.
    pub fn selects(&self, index: usize) -> bool {
        self.only.as_ref().is_none_or(|only| only.contains(&index))
    }

    /// Refuse a selection that runs nothing or names an index past the
    /// stage's list: either would report a pass for criteria that never ran.
    pub(crate) fn check_selection(&self, stage: &Stage) -> Result<()> {
        let Some(only) = &self.only else {
            return Ok(());
        };
        ensure!(!only.is_empty(), "the criterion selection is empty");
        let len = stage.acceptance.len();
        if let Some(index) = only.range(len..).next() {
            bail!(
                "criterion {index} is out of range: stage '{}' has {len} acceptance criteria, \
                 numbered from 0",
                stage.id
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::AcceptanceCriterion;

    #[test]
    fn a_selection_past_the_list_or_empty_is_refused() {
        let mut stage = Stage::new("subset".to_string(), None);
        stage.add_acceptance_criterion(AcceptanceCriterion::Simple("true".to_string()));
        stage.add_acceptance_criterion(AcceptanceCriterion::Simple("true".to_string()));
        let select = |only: &[usize]| {
            CriteriaConfig::default().with_only(Some(only.iter().copied().collect()))
        };

        let error = select(&[1, 2]).check_selection(&stage).unwrap_err();
        assert!(
            error.to_string().contains("criterion 2 is out of range"),
            "{error}"
        );
        assert!(
            error.to_string().contains("has 2 acceptance criteria"),
            "{error}"
        );
        assert!(select(&[]).check_selection(&stage).is_err());
        assert!(select(&[0, 1]).check_selection(&stage).is_ok());
        assert!(CriteriaConfig::default().check_selection(&stage).is_ok());
    }
}
