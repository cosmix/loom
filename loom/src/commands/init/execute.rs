//! Main execution entry point for loom init command.

use crate::commands::common::resolve_state_dir;
use crate::commands::repair::workspace::{repair_workspace, AppliedRepair};
use crate::fs::permissions::{ensure_loom_permissions, migrate_legacy_trust};
use crate::fs::work_dir::WorkDir;
use crate::fs::work_integrity::validate_work_dir_state;
use anyhow::{bail, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use super::backend::resolve_backend_choice;
use super::cleanup::{
    cleanup_orphaned_sessions, cleanup_work_directory, cleanup_worktrees_directory,
    prune_stale_worktrees, remove_work_directory_on_failure, SessionReapMode,
};
use super::git_hooks;
use super::plan_setup::{initialize_with_plan, preflight_plan, PreflightedPlan};
use crate::fs::work_state::holds_orchestration_state;

/// RAII guard that cleans up the state directory on drop unless disarmed.
/// This ensures cleanup happens on ANY failure path, not just plan parsing.
struct InitGuard {
    repo_root: PathBuf,
    work_created: bool,
    disarmed: bool,
}

impl InitGuard {
    fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            work_created: false,
            disarmed: false,
        }
    }

    fn mark_work_created(&mut self) {
        self.work_created = true;
    }

    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for InitGuard {
    fn drop(&mut self) {
        if self.work_created && !self.disarmed {
            let work_dir = crate::commands::common::resolve_state_dir(&self.repo_root);
            println!(
                "  {} Cleaning up {} due to initialization failure",
                "→".yellow().bold(),
                work_dir.display().to_string().dimmed()
            );
            remove_work_directory_on_failure(&self.repo_root);
        }
    }
}

/// Initialize the state directory structure
///
/// # Arguments
/// * `plan_path` - Optional path to a plan file to initialize with
/// * `clean` - If true, clean up stale resources before initialization
/// * `backend` - Terminal backend for sessions (native|tmux); `None` defers
///   to `~/.loom/config.toml` (a TTY prompt lets Enter inherit that value or
///   pin a typed choice)
/// * `no_repair` - Skip the automatic workspace repair pass
pub fn execute(
    plan_path: Option<PathBuf>,
    clean: bool,
    backend: Option<String>,
    no_repair: bool,
) -> Result<()> {
    let repo_root = std::env::current_dir()?;
    for state in [repo_root.join(".loom/work"), repo_root.join(".work")] {
        if holds_orchestration_state(&state) {
            crate::git::worktree::require_main_identity_at(&repo_root, &state)?;
        }
    }
    let preflighted = preflight_plan_if_given(plan_path.as_deref())?;
    prepare_workspace(&repo_root, clean, no_repair)?;
    let (work_dir, mut guard) = create_init_work_dir(&repo_root)?;
    if crate::git::worktree::main_identity_exists(work_dir.root()) {
        crate::git::worktree::require_main_identity_at(&repo_root, work_dir.root())?;
    } else {
        crate::git::worktree::capture_main_identity(&repo_root, work_dir.root())?;
    }
    configure_workspace(&repo_root)?;

    if let Some(preflighted) = preflighted {
        let backend = resolve_backend_choice(backend)?;
        let stage_count = initialize_with_plan(&work_dir, &preflighted, &repo_root, backend)?;
        print_summary(plan_path.as_deref(), stage_count);
    } else {
        print_summary(None, 0);
    }

    // Publish for both planned and plan-less init; a dirty first checkout
    // may need the source graph's overlay fallback.
    crate::commands::run::checks::advisory_source_graph_preflight(&repo_root, &work_dir);
    guard.disarm();
    Ok(())
}

fn prepare_workspace(repo_root: &Path, clean: bool, no_repair: bool) -> Result<()> {
    let has_state = [repo_root.join(".loom/work"), repo_root.join(".work")]
        .iter()
        .any(|state| {
            holds_orchestration_state(state) || crate::git::worktree::main_identity_exists(state)
        });
    let repo_bootstrap = if has_state {
        crate::git::repository::ensure_pinned_repo_ready(repo_root)?
    } else {
        crate::git::ensure_repo_ready_for_worktrees(repo_root)?
    };

    // Repair the workspace BEFORE the validator judges it - see `startup_repairs`.
    let repairs = startup_repairs(repo_root, no_repair)?;

    validate_work_dir_state(repo_root)?;

    print_header();
    print_repo_bootstrap(repo_bootstrap);
    for line in startup_repair_lines(&repairs) {
        println!("{line}");
    }

    println!("\n{}", "Cleanup".bold());
    println!("{}", "─".repeat(40).dimmed());
    cleanup_previous_run(repo_root, clean)?;

    println!("\n{}", "Initialize".bold());
    println!("{}", "─".repeat(40).dimmed());
    Ok(())
}

fn create_init_work_dir(repo_root: &Path) -> Result<(WorkDir, InitGuard)> {
    // `loom init` is one-shot: if the state directory already holds real orchestration
    // state, refuse. Pass --clean to wipe and start over. Reusing a stateful
    // state directory would silently overlay the new plan's stages on top of the
    // previous plan's files, producing duplicate ids and an unrecoverable
    // graph. A state directory that merely EXISTS but holds no such state — e.g. one a
    // stale LOOM_WORK_DIR pin made a hook materialize against a since-deleted
    // state directory (see `WorkDir::new`) — is adopted instead of refused.
    let work_dir = WorkDir::new(".")?;
    let work_dir_path = work_dir.root();
    let adopting_empty_work_dir =
        work_dir_path.exists() && !holds_orchestration_state(work_dir_path);
    if work_dir_path.exists() && !adopting_empty_work_dir {
        bail!(
            "{} already initialized.\n\
             Run `loom init <plan> --clean` to wipe existing state and start over,\n\
             or `loom clean` followed by `loom init <plan>`.",
            work_dir_path.display()
        );
    }

    let mut guard = InitGuard::new(repo_root.to_path_buf());
    create_or_adopt_work_dir(&work_dir, adopting_empty_work_dir, &mut guard)?;
    Ok((work_dir, guard))
}

fn configure_workspace(repo_root: &Path) -> Result<()> {
    git_hooks::install_git_hooks(repo_root);

    ensure_loom_permissions(repo_root)?;
    println!("  {} Permissions configured", "✓".green().bold());
    install_codex_hooks_advisory();

    if let Ok(paths) = crate::assets::install::default_paths() {
        let rules = paths.claude_dir.join("CLAUDE.md");
        if !rules.exists() {
            println!("  {} {} not found", "!".yellow().bold(), rules.display());
            println!(
                "    {}",
                "Run install.sh or loom update to install loom rules.".dimmed()
            );
        }
    }

    // Clean up legacy trustedDirectories entries (no-op if none exist)
    if let Err(e) = migrate_legacy_trust(repo_root) {
        eprintln!("  {} Legacy trust migration: {}", "!".yellow().bold(), e);
    }
    Ok(())
}

fn cleanup_previous_run(repo_root: &Path, clean: bool) -> Result<()> {
    if clean {
        let work_dir = resolve_state_dir(repo_root);
        crate::fs::tmux_tmpdir::adopt_recorded_tmux_tmpdir(&work_dir);
    }
    stop_daemon_and_prune(repo_root, clean)?;
    // `--clean` deletes the state directory and its session records. Reap
    // attributed sockets while those records can still identify them.
    let session_reap_mode = if clean {
        SessionReapMode::IncludeLiveBeforeClean
    } else {
        SessionReapMode::OrphansOnly
    };
    cleanup_orphaned_sessions(repo_root, session_reap_mode)?;

    if clean {
        let work_dir = resolve_state_dir(repo_root);
        crate::orchestrator::terminal::tmux::viewer::retire_viewer(&work_dir)?;
        cleanup_work_directory(repo_root)?;
        cleanup_worktrees_directory(repo_root)?;
    }
    Ok(())
}

/// Stop a running daemon before `--clean` deletes the state directory
/// further down; its singleton flock would otherwise survive the
/// directory's deletion and recreation and keep ticking over the next
/// plan's files. Then prune worktrees left behind by a previous run.
fn stop_daemon_and_prune(repo_root: &Path, clean: bool) -> Result<()> {
    if clean {
        // Before the daemon stops and sessions are reaped: deleting the target
        // guard record would make the next run trust an unreviewed move.
        crate::commands::target::refuse_unreviewed_move(repo_root)?;
        crate::commands::stop::ensure_daemon_stopped(&resolve_state_dir(repo_root))?;
    }
    prune_stale_worktrees(repo_root)
}

fn install_codex_hooks_advisory() {
    match crate::fs::permissions::install_codex_hooks() {
        Ok(count) => println!(
            "  {} Codex hooks configured ({count} assets; review changes with /hooks)",
            "✓".green().bold()
        ),
        Err(error) => eprintln!(
            "  {} Codex hook setup skipped: {}",
            "!".yellow().bold(),
            error
        ),
    }
}

/// Parse and policy-check the plan, when one was given, before `execute`
/// creates or writes anything.
///
/// The ordering matters: `InitGuard` only ever removes the state directory, so
/// a plan that fails any later leaves the git hooks and the
/// `.claude/settings.local.json` edits behind, and makes the operator answer
/// the backend prompt a second time.
fn preflight_plan_if_given(plan_path: Option<&Path>) -> Result<Option<PreflightedPlan>> {
    plan_path.map(preflight_plan).transpose()
}

/// Repair the workspace before anything judges it.
///
/// Placement is load-bearing: the caller runs this AFTER
/// `ensure_repo_ready_for_worktrees` (a workspace can only be repaired inside a
/// real repository) and BEFORE `validate_work_dir_state`, which bails on a
/// `.loom/work` symlink in the main repo - the exact failure this pass heals.
/// Called after the validator, it would never run on that case.
///
/// `no_repair` is the `--no-repair` opt-out: no checks, no fixes, nothing
/// touched.
pub(super) fn startup_repairs(repo_root: &Path, no_repair: bool) -> Result<Vec<AppliedRepair>> {
    if no_repair {
        return Ok(Vec::new());
    }
    repair_workspace(repo_root)
}

/// One line per repair applied - and NOTHING at all when none was. `loom init`
/// is run constantly, and a repair banner on every invocation trains people to
/// ignore it.
pub(super) fn startup_repair_lines(applied: &[AppliedRepair]) -> Vec<String> {
    applied
        .iter()
        .map(|repair| format!("  {} Repaired: {}", "✓".green().bold(), repair.description))
        .collect()
}

/// Lay out the state directory, either creating it or adopting a stateless
/// one that is already there.
///
/// An adopted directory is deliberately NOT marked on the guard: `InitGuard`
/// deletes the state directory wholesale on a later failure, and it may only
/// delete a directory THIS run created.
fn create_or_adopt_work_dir(
    work_dir: &WorkDir,
    adopting: bool,
    guard: &mut InitGuard,
) -> Result<()> {
    let label = work_dir.root().display().to_string();
    if adopting {
        work_dir.adopt_existing()?;
        println!(
            "  {} Adopted existing empty {}",
            "✓".green().bold(),
            label.dimmed()
        );
        println!(
            "    {}",
            format!("Derived caches ({label}/context/, {label}/.loom/) were left in place.")
                .dimmed()
        );
    } else {
        work_dir.initialize()?;
        guard.mark_work_created();
        println!(
            "  {} Directory structure created {}",
            "✓".green().bold(),
            label.dimmed()
        );
    }
    warn_on_long_socket_path(work_dir.root());
    Ok(())
}

/// Warn when the daemon could not bind its socket under `work_dir_path`, so a
/// repository path that is too long surfaces at init rather than at `loom run`.
/// Returns the warning it printed.
fn warn_on_long_socket_path(work_dir_path: &Path) -> Option<String> {
    let problem = crate::daemon::socket_path_problem(work_dir_path)?;
    println!("  {} {problem}", "!".yellow().bold());
    Some(problem)
}

fn print_repo_bootstrap(repo_bootstrap: crate::git::RepoBootstrapResult) {
    if !repo_bootstrap.changed() {
        return;
    }

    println!("\n{}", "Git".bold());
    println!("{}", "─".repeat(40).dimmed());

    if repo_bootstrap.initialized_repo {
        println!("  {} Initialized git repository", "✓".green().bold());
    }

    if repo_bootstrap.created_initial_commit {
        println!(
            "  {} Created bootstrap commit for worktree support",
            "✓".green().bold()
        );
    }

    if repo_bootstrap.removed_stale_git_locks {
        println!(
            "  {} Removed stale .git lock files that blocked git init",
            "✓".green().bold()
        );
    }

    if repo_bootstrap.backed_up_git_config {
        println!(
            "  {} Moved unreadable .git/config aside (kept as .git/config.loom-backup-*)",
            "!".yellow().bold()
        );
    }
}

/// Print the loom init header
fn print_header() {
    crate::utils::print_logo_header("Initializing...");
}

/// Print the final summary
fn print_summary(plan_path: Option<&Path>, stage_count: usize) {
    println!();
    println!("{}", "═".repeat(40).dimmed());

    if let Some(path) = plan_path {
        println!(
            "{} Initialized from {}",
            "✓".green().bold(),
            path.display().to_string().cyan()
        );
        println!(
            "  {} stage{} ready for execution",
            stage_count.to_string().bold(),
            if stage_count == 1 { "" } else { "s" }
        );
    } else {
        println!("{} Empty workspace initialized", "✓".green().bold());
    }

    println!();
    println!("{}", "Next steps:".bold());
    println!("  {}  Start execution", "loom run".cyan());
    println!("  {}  View dashboard", "loom status".cyan());
    println!();
}

#[cfg(test)]
mod tests;
