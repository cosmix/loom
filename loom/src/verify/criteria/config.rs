//! Configuration types for acceptance criteria execution

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::models::stage::CommandConfinement;

use super::cache::CachePolicy;

/// Default timeout for command execution (5 minutes)
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// Default timeout for an extended acceptance criterion or a truth check
/// that sets no `timeout_secs` (30 seconds)
pub const DEFAULT_CHECK_TIMEOUT: Duration = Duration::from_secs(30);

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
}

impl Default for CriteriaConfig {
    fn default() -> Self {
        Self {
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
            plan_confinement: None,
            cache: CachePolicy::from_env(),
            cache_dir: None,
            tree_root: None,
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
}
