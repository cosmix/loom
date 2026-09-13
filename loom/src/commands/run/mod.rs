//! Run command - execute plan stages via orchestrator.
//!
//! This module provides commands for running loom plans either in foreground
//! (debugging) or background (daemon) mode.

pub(crate) mod checks;
mod foreground;
mod graph_loader;
mod plan_inputs;
mod sandbox_preflight;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_checks;

use anyhow::{bail, Result};
use colored::Colorize;

use crate::daemon::{DaemonConfig, DaemonServer};
use crate::fs::work_dir::{read_terminal_config, write_terminal_config, WorkDir};
use crate::models::session::{SessionBackendKind, TerminalConfig};

use checks::prepare_repo_for_run;

// Re-export the main entry point for foreground mode
pub use foreground::execute;

// Re-export plan lifecycle functions for daemon use (now from fs module)
pub use crate::fs::plan_lifecycle::mark_plan_done_if_all_merged;

/// Execute orchestrator in background (daemon mode)
/// Usage: `loom run [--manual] [--max-parallel <n>] [--no-merge] [--backend <native|tmux>]`
pub fn execute_background(
    manual: bool,
    max_parallel: Option<usize>,
    auto_merge: bool,
    backend: Option<String>,
) -> Result<()> {
    let work_dir = prepare_background_run(backend)?;

    crate::utils::print_logo_header("Run");

    if DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is already running", "─".dimmed());
        println!();
        println!("  {}  Check status", "loom status".cyan());
        print_stop_guidance();
        return Ok(());
    }

    // Detect terminal BEFORE daemonizing (daemon loses terminal context after fork)
    // Store in environment variable so it can be read back after the fork
    if let Ok(terminal) = crate::orchestrator::terminal::native::detect_terminal() {
        // SAFETY: This runs in main() before the tokio runtime spawns any threads,
        // so there are no concurrent readers of the environment.
        unsafe { std::env::set_var("LOOM_TERMINAL", terminal.display_name()) };
    }

    let daemon_config = DaemonConfig {
        manual_mode: manual,
        max_parallel,
        watch_mode: true, // Background daemon mode continuously watches by design.
        auto_merge,
    };

    let daemon = DaemonServer::with_config(work_dir.root(), daemon_config);
    daemon.start()?;

    println!("{} Daemon started", "✓".green().bold());
    if !auto_merge {
        println!("  {} Auto-merge disabled", "→".dimmed());
    }
    println!();
    println!("  {}  Monitor progress", "loom status".cyan());
    print_stop_guidance();

    Ok(())
}

fn prepare_background_run(backend: Option<String>) -> Result<WorkDir> {
    // Ensure git worktree prerequisites are met before starting.
    let repo_root = std::env::current_dir()?;
    prepare_repo_for_run(&repo_root)?;

    // Absolute, not ".": a long-lived daemon must get an absolute base — a
    // relative state-directory root silently diverges from every other
    // process's view of the same paths (e.g. `loom attach`, which always
    // resolves absolute).
    let work_dir = WorkDir::new(&repo_root)?;
    work_dir.load()?;

    plan_inputs::require_committed_plan(&work_dir)?;

    resolve_backend_flag(&work_dir, backend, "loom run")?;

    // Hard requirement — like `require_jq`: a missing sandbox prerequisite on
    // Linux/WSL makes every session exit at startup, so fail here instead of
    // burning the retry budget on a deterministic startup refusal.
    sandbox_preflight::require_sandbox_prerequisites(work_dir.root())?;

    // Advisory Remote Control preflight — never aborts startup.
    if let Ok(claude_path) = crate::claude::find_claude_path() {
        crate::remote_control::run_startup_preflight(&claude_path, work_dir.root());
    }

    // Advisory Codex lane preflight — never aborts startup.
    checks::advisory_codex_lane_preflight(work_dir.root());

    plan_inputs::mark_plan_in_progress(&work_dir)?;

    // Publish against the committed active filename and the revision stages inherit.
    checks::advisory_source_graph_preflight(&repo_root, &work_dir);

    Ok(work_dir)
}

fn print_stop_guidance() {
    println!("  {}  Stop daemon", "loom stop".cyan());
}

/// Resolve `--backend`, persisting an explicit selection, then run the tmux
/// preflight. Shared by `loom run` (daemon mode, see [`execute_background`])
/// and `loom run --foreground` (see [`foreground::execute`]) — this is the
/// ONLY path that runs the preflight, so a fix here reaches both callers
/// instead of risking a fix landing in one copy and not the other.
///
/// Guards against desync with an already-running daemon: its backend is
/// fixed at construction, so a config flip alone cannot reach it. `loom run`
/// never prompts — only `loom init` does.
///
/// `invocation` is the exact command text the caller was invoked as (e.g.
/// `"loom run"` or `"loom run --foreground"`). The desync hint below tells
/// the operator to re-run with `--backend <value>` appended to THIS text —
/// each call site supplies its own so a foreground invocation is never told
/// to drop `--foreground` and run a different command than the one it typed.
///
/// When `backend` is `None`, nothing is persisted; only the preflight check
/// (against whatever backend is already persisted) still runs.
fn resolve_backend_flag(
    work_dir: &WorkDir,
    backend: Option<String>,
    invocation: &str,
) -> Result<()> {
    resolve_backend_flag_with_probe(work_dir, backend, invocation, || {
        which::which("tmux").is_ok()
    })
}

/// [`resolve_backend_flag`] with the tmux-availability probe injectable, so
/// the hard-failure preflight below is unit testable without depending on
/// whether the host actually has tmux on PATH.
fn resolve_backend_flag_with_probe(
    work_dir: &WorkDir,
    backend: Option<String>,
    invocation: &str,
    tmux_available: impl Fn() -> bool,
) -> Result<()> {
    if let Some(value) = backend {
        let requested = match value.as_str() {
            "native" => SessionBackendKind::Native,
            "tmux" => SessionBackendKind::Tmux,
            other => bail!("Invalid terminal backend: {other}"),
        };

        let persisted = read_terminal_config(work_dir.root())?.backend;

        if DaemonServer::is_running(work_dir.root()) && requested != persisted {
            println!(
                "{} {}",
                "─".dimmed(),
                backend_restart_hint(invocation, &value)
            );
        } else {
            write_terminal_config(work_dir.root(), &TerminalConfig { backend: requested })?;
        }
    }

    // Hard requirement — like `require_sandbox_prerequisites` above: a
    // configured tmux backend with no tmux on PATH makes every spawn fail
    // deterministically, so stop here instead of starting the daemon into a
    // guaranteed failure.
    if read_terminal_config(work_dir.root())?.backend == SessionBackendKind::Tmux
        && !tmux_available()
    {
        bail!(
            "terminal backend \"tmux\" is configured but tmux is not on PATH; install tmux or \
             set [terminal] backend = \"native\" (.loom/work/config.toml or ~/.loom/config.toml)"
        );
    }

    Ok(())
}

/// Builds the daemon-desync hint text for [`resolve_backend_flag`], isolated
/// from the `println!` so the FIX-critical part — re-running the SAME
/// command shape the operator actually invoked, not a different one — is
/// unit-testable without a live daemon (see `backend_flag_tests` below for
/// why the branch that calls this is not).
fn backend_restart_hint(invocation: &str, value: &str) -> String {
    format!(
        "backend change requires a daemon restart: run `loom stop`, then `{invocation} --backend {value}`"
    )
}

/// Tests for [`resolve_backend_flag`] — the shared `--backend` resolution and
/// marker-clearing logic used by both `loom run` and `loom run --foreground`.
///
/// Kept inline (rather than in `run/tests.rs`) since these exercise a
/// module-private function; none of these need a live daemon, so the
/// daemon-desync branch (`DaemonServer::is_running(..) && requested !=
/// persisted`) is intentionally not covered here — it would require an
/// actually-running daemon process, which is out of scope for a unit test.
#[cfg(test)]
mod backend_flag_tests {
    use super::{resolve_backend_flag, resolve_backend_flag_with_probe};
    use crate::fs::work_dir::{read_terminal_config, write_terminal_config, WorkDir};
    use crate::models::session::{SessionBackendKind, TerminalConfig};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn tmux_selection_persists_tmux_when_tmux_is_available() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        fs::create_dir_all(work_dir.root()).unwrap();

        resolve_backend_flag_with_probe(&work_dir, Some("tmux".to_string()), "loom run", || true)
            .unwrap();

        assert_eq!(
            read_terminal_config(work_dir.root()).unwrap().backend,
            SessionBackendKind::Tmux
        );
    }

    #[test]
    fn native_selection_persists_native() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        fs::create_dir_all(work_dir.root()).unwrap();

        resolve_backend_flag(&work_dir, Some("native".to_string()), "loom run").unwrap();

        assert_eq!(
            read_terminal_config(work_dir.root()).unwrap().backend,
            SessionBackendKind::Native
        );
    }

    #[test]
    fn invalid_backend_value_errors_without_touching_config() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        fs::create_dir_all(work_dir.root()).unwrap();
        write_terminal_config(
            work_dir.root(),
            &TerminalConfig {
                backend: SessionBackendKind::Native,
            },
        )
        .unwrap();

        let result = resolve_backend_flag(&work_dir, Some("screen".to_string()), "loom run");

        assert!(result.is_err());
        assert_eq!(
            read_terminal_config(work_dir.root()).unwrap().backend,
            SessionBackendKind::Native,
            "an invalid --backend value must never touch the persisted config"
        );
    }

    #[test]
    fn omitted_backend_flag_writes_nothing() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        fs::create_dir_all(work_dir.root()).unwrap();
        let config_path = work_dir.root().join("config.toml");
        assert!(!config_path.exists());

        resolve_backend_flag(&work_dir, None, "loom run").unwrap();

        assert!(
            !config_path.exists(),
            "omitting --backend must not write config.toml"
        );
    }

    #[test]
    fn tmux_configured_and_unavailable_fails_the_preflight() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        fs::create_dir_all(work_dir.root()).unwrap();
        write_terminal_config(
            work_dir.root(),
            &TerminalConfig {
                backend: SessionBackendKind::Tmux,
            },
        )
        .unwrap();

        let err = resolve_backend_flag_with_probe(&work_dir, None, "loom run", || false)
            .expect_err("a configured tmux backend with no tmux on PATH must fail the preflight");

        let rendered = err.to_string();
        assert!(rendered.contains("tmux is not on PATH"), "{rendered}");
        assert!(
            rendered.contains("[terminal] backend = \"native\""),
            "must name the fix, got: {rendered}"
        );
    }

    #[test]
    fn tmux_configured_and_available_passes_the_preflight() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = WorkDir::new(temp_dir.path()).unwrap();
        fs::create_dir_all(work_dir.root()).unwrap();
        write_terminal_config(
            work_dir.root(),
            &TerminalConfig {
                backend: SessionBackendKind::Tmux,
            },
        )
        .unwrap();

        resolve_backend_flag_with_probe(&work_dir, None, "loom run", || true)
            .expect("a configured tmux backend with tmux on PATH must pass the preflight");
    }

    #[test]
    fn backend_restart_hint_uses_the_callers_invocation_text() {
        // The daemon-desync branch itself needs a live daemon (see the
        // module doc comment above), but the text it prints is a pure
        // function of `invocation` and is exactly what FIX 3 is about: a
        // foreground caller must be told to re-run `loom run --foreground
        // --backend <x>`, not `loom run --backend <x>`.
        assert_eq!(
            super::backend_restart_hint("loom run", "tmux"),
            "backend change requires a daemon restart: run `loom stop`, then `loom run --backend tmux`"
        );
        assert_eq!(
            super::backend_restart_hint("loom run --foreground", "native"),
            "backend change requires a daemon restart: run `loom stop`, then `loom run --foreground \
             --backend native`"
        );
    }
}
