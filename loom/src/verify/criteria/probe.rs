//! One command run through the acceptance-criteria runner, cache included:
//! the same `CommandSpec` a criterion carrying the command would get, so an
//! identical earlier pass is reused from the certified cache.

use anyhow::{Context, Result};
use std::path::Path;

use super::{run_acceptance_with_config, CriteriaConfig};
use crate::models::stage::{AcceptanceCriterion, Stage};

/// What one command printed and how it ended.
pub(in crate::verify) struct ProbeRun {
    pub(in crate::verify) stdout: String,
    pub(in crate::verify) stderr: String,
    pub(in crate::verify) exit_code: Option<i32>,
    pub(in crate::verify) timed_out: bool,
    /// Set when the run is a certified cache hit: `stdout` and `stderr` are
    /// then tails of the original run, too short to re-parse.
    pub(in crate::verify) cached: Option<CachedRun>,
}

/// What a certified cache hit kept of the original run's test counts.
pub(in crate::verify) struct CachedRun {
    /// Tests the original run executed, as the zero-test guard counted them.
    pub(in crate::verify) tests_executed: Option<u64>,
}

/// Runs a command with `package_dir` as its cwd.
pub(in crate::verify) trait ProbeRunner {
    fn run(&self, command: &str, package_dir: &Path) -> Result<ProbeRun>;
}

/// The acceptance-criteria runner, cache included. A command starting in a
/// package directory may use all of `config.tree_root`, the stage's worktree.
pub(in crate::verify) struct CriteriaProbe<'a> {
    pub(in crate::verify) stage: &'a Stage,
    pub(in crate::verify) config: CriteriaConfig,
}

impl ProbeRunner for CriteriaProbe<'_> {
    fn run(&self, command: &str, package_dir: &Path) -> Result<ProbeRun> {
        let probe = Stage {
            acceptance: vec![AcceptanceCriterion::Simple(command.to_string())],
            ..self.stage.clone()
        };
        let result = run_acceptance_with_config(&probe, Some(package_dir), &self.config)?;
        let run = result
            .results()
            .first()
            .context("the criteria runner returned no result")?;
        Ok(ProbeRun {
            stdout: run.stdout.clone(),
            stderr: run.stderr.clone(),
            exit_code: run.exit_code,
            timed_out: run.timed_out,
            cached: run.cached.then_some(CachedRun {
                tests_executed: run.cached_tests_executed,
            }),
        })
    }
}
