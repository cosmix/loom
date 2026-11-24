//! One command run through the acceptance-criteria runner, cache included:
//! the same `CommandSpec` a criterion carrying the command would get, so an
//! identical earlier pass is reused from the certified cache.

use anyhow::{Context, Result};
use std::path::Path;

use super::{run_acceptance_with_config, CriteriaConfig};
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::verify::environment::refused_bind_note;

/// What one command printed and how it ended.
pub(in crate::verify) struct ProbeRun {
    pub(in crate::verify) stdout: String,
    pub(in crate::verify) stderr: String,
    pub(in crate::verify) exit_code: Option<i32>,
    pub(in crate::verify) timed_out: bool,
    /// Whether both streams were captured completely. Cached diagnostic tails
    /// are certified separately by `cached` and retain this original verdict.
    pub(in crate::verify) output_complete: bool,
    /// Set when the run is a certified cache hit: `stdout` and `stderr` are
    /// then tails of the original run, too short to re-parse.
    pub(in crate::verify) cached: Option<CachedRun>,
    /// The command was confined on macOS with no loopback networking.
    pub(in crate::verify) loopback_refused: bool,
}

impl ProbeRun {
    /// The note naming the loopback opt-in when this failed run printed a
    /// refused bind (`environment::refused_bind_note`), read from the whole
    /// output.
    pub(in crate::verify) fn bind_note(&self) -> Option<String> {
        refused_bind_note(
            self.loopback_refused,
            self.timed_out,
            self.exit_code,
            &self.stdout,
            &self.stderr,
        )
    }

    /// A completed, uncached run with its loopback available.
    #[cfg(test)]
    pub(in crate::verify) fn finished(
        stdout: impl Into<String>,
        stderr: impl Into<String>,
        exit_code: Option<i32>,
    ) -> Self {
        Self {
            stdout: stdout.into(),
            stderr: stderr.into(),
            exit_code,
            timed_out: false,
            output_complete: true,
            cached: None,
            loopback_refused: false,
        }
    }
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
            output_complete: run.output_complete,
            cached: run.cached.then_some(CachedRun {
                tests_executed: run.cached_tests_executed,
            }),
            loopback_refused: run.loopback_refused,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::{CommandConfinement, NetworkConfig};

    /// Prints a refused bind on stderr and fails, as a server that cannot listen.
    const REFUSED: &str = "printf 'code: \"EADDRINUSE\"\\n' >&2; exit 1";

    /// `REFUSED` through the real criteria runner under `stage` and `config`;
    /// `None` when no confinement launcher runs on this host.
    fn refused_run(stage: &Stage, config: CriteriaConfig) -> Option<ProbeRun> {
        let temp = tempfile::TempDir::new().unwrap();
        let probe = CriteriaProbe { stage, config };
        probe.run(REFUSED, temp.path()).ok()
    }

    #[test]
    fn bind_refusal_denied_run_is_flagged_on_macos_only() {
        let Some(run) = refused_run(&Stage::default(), CriteriaConfig::default()) else {
            return;
        };
        assert_eq!(run.loopback_refused, cfg!(target_os = "macos"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bind_refusal_macos_kernel_confined_run_carries_the_note() {
        let name = "bind_refusal_macos_kernel_confined_run_carries_the_note";
        if crate::process::sandbox_probe::skip_unless(
            crate::process::sandbox_probe::confined_commands_runnable(),
            name,
            "confined commands cannot start",
        ) {
            return;
        }
        let run = refused_run(&Stage::default(), CriteriaConfig::default())
            .expect("a confined command starts when the launcher is runnable");
        assert_eq!(run.exit_code, Some(1));
        assert!(run.loopback_refused);
        let note = run.bind_note();
        assert!(
            note.as_deref()
                .is_some_and(|note| note.contains("allow_local_binding")),
            "{note:?}"
        );
    }

    #[test]
    fn bind_refusal_is_absent_when_opted_in_or_inherited() {
        let mut opted_in = Stage::default();
        opted_in.sandbox.network = Some(NetworkConfig {
            allow_local_binding: true,
            ..NetworkConfig::default()
        });
        let mut inherited = Stage::default();
        inherited.sandbox.command_confinement = Some(CommandConfinement::Inherit);
        let plan_opt_in = CriteriaConfig::default().with_plan_local_binding(true);
        let cases = [
            (opted_in, CriteriaConfig::default()),
            (Stage::default(), plan_opt_in),
            (inherited, CriteriaConfig::default()),
        ];
        for (stage, config) in cases {
            let Some(run) = refused_run(&stage, config) else {
                continue;
            };
            assert!(!run.loopback_refused);
            assert_eq!(run.bind_note(), None);
        }
    }

    #[test]
    fn probe_preserves_executor_capture_incompleteness() {
        if !crate::process::sandbox_probe::confined_commands_runnable() {
            return;
        }
        let temp = tempfile::TempDir::new().unwrap();
        let stage = Stage::default();
        let probe = CriteriaProbe {
            stage: &stage,
            config: CriteriaConfig::default(),
        };
        let run = probe
            .run(
                "awk 'BEGIN { for (i=0; i<1200000; i++) print \"123456789\" }'",
                temp.path(),
            )
            .unwrap();
        assert!(
            !run.output_complete,
            "exit={:?}, stdout bytes={}, stderr={}",
            run.exit_code,
            run.stdout.len(),
            run.stderr
        );
        assert!(run.cached.is_none());
    }
}
