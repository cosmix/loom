//! Main execution entry point for loom init command.

use crate::fs::permissions::{ensure_loom_permissions, migrate_legacy_trust};
use crate::fs::work_dir::WorkDir;
use crate::fs::work_integrity::validate_work_dir_state;
use crate::git::install_pre_commit_hook;
use crate::models::session::SessionBackendKind;
use anyhow::{bail, Result};
use colored::Colorize;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};

use super::cleanup::{
    cleanup_orphaned_sessions, cleanup_work_directory, cleanup_worktrees_directory,
    prune_stale_worktrees, remove_work_directory_on_failure, SessionReapMode,
};
use super::plan_setup::initialize_with_plan_acknowledgement;

/// RAII guard that cleans up .work directory on drop unless disarmed.
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
            println!(
                "  {} Cleaning up {} due to initialization failure",
                "→".yellow().bold(),
                ".work/".dimmed()
            );
            remove_work_directory_on_failure(&self.repo_root);
        }
    }
}

/// Initialize the .work/ directory structure
///
/// # Arguments
/// * `plan_path` - Optional path to a plan file to initialize with
/// * `clean` - If true, clean up stale resources before initialization
/// * `backend` - Terminal backend for sessions (native|tmux); `None` prompts
///   interactively on a TTY, or defaults to native otherwise
pub fn execute(
    plan_path: Option<PathBuf>,
    clean: bool,
    backend: Option<String>,
    allow_unsafe_plan: bool,
) -> Result<()> {
    let repo_root = std::env::current_dir()?;
    let repo_bootstrap = crate::git::ensure_repo_ready_for_worktrees(&repo_root)?;

    // Validate .work directory state before proceeding
    validate_work_dir_state(&repo_root)?;

    print_header();

    print_repo_bootstrap(repo_bootstrap);

    println!("\n{}", "Cleanup".bold());
    println!("{}", "─".repeat(40).dimmed());

    prune_stale_worktrees(&repo_root)?;
    // `--clean` is about to delete `.work/` below, which destroys the ONLY
    // record (`.work/sessions/<id>.md`) that lets a tmux socket ever be
    // attributed to this work dir again. Reap attributed sockets EVEN IF
    // their session is still alive in that case; otherwise stay
    // conservative and only reap truly-dead sessions.
    let session_reap_mode = if clean {
        SessionReapMode::IncludeLiveBeforeClean
    } else {
        SessionReapMode::OrphansOnly
    };
    cleanup_orphaned_sessions(&repo_root, session_reap_mode)?;

    if clean {
        cleanup_work_directory(&repo_root)?;
        cleanup_worktrees_directory(&repo_root)?;
    }

    println!("\n{}", "Initialize".bold());
    println!("{}", "─".repeat(40).dimmed());

    // `loom init` is one-shot: if .work/ already exists, refuse. Pass --clean
    // to wipe and start over. Reusing an existing .work/ would silently
    // overlay the new plan's stages on top of the previous plan's files,
    // producing duplicate ids and an unrecoverable graph.
    if repo_root.join(".work").exists() {
        bail!(
            ".work/ already initialized.\n\
             Run `loom init <plan> --clean` to wipe existing state and start over,\n\
             or `loom clean` followed by `loom init <plan>`."
        );
    }

    let mut guard = InitGuard::new(repo_root.clone());
    let work_dir = WorkDir::new(".")?;
    work_dir.initialize()?;
    guard.mark_work_created();
    println!(
        "  {} Directory structure created {}",
        "✓".green().bold(),
        ".work/".dimmed()
    );

    // Install git pre-commit hook to prevent .work commits
    match install_pre_commit_hook(&repo_root) {
        Ok(true) => {
            println!("  {} Git pre-commit hook installed", "✓".green().bold());
        }
        Ok(false) => {
            println!(
                "  {} Git pre-commit hook {} up to date",
                "✓".green().bold(),
                "already".dimmed()
            );
        }
        Err(e) => {
            println!(
                "  {} Git pre-commit hook installation failed: {}",
                "!".yellow().bold(),
                e.to_string().dimmed()
            );
            // Non-fatal - continue with init
        }
    }

    ensure_loom_permissions(&repo_root)?;
    println!("  {} Permissions configured", "✓".green().bold());

    // Check for CLAUDE.md
    if let Some(home) = dirs::home_dir() {
        let claude_md = home.join(".claude/CLAUDE.md");
        if !claude_md.exists() {
            println!("  {} ~/.claude/CLAUDE.md not found", "!".yellow().bold());
            println!(
                "    {}",
                "Run install.sh or loom self-update to install loom rules.".dimmed()
            );
        }
    }

    // Clean up legacy trustedDirectories entries (no-op if none exist)
    if let Err(e) = migrate_legacy_trust(&repo_root) {
        eprintln!("  {} Legacy trust migration: {}", "!".yellow().bold(), e);
    }

    if let Some(path) = plan_path {
        let terminal_backend = resolve_backend_choice(backend)?;
        let stage_count = initialize_with_plan_acknowledgement(
            &work_dir,
            &path,
            terminal_backend,
            allow_unsafe_plan,
        )?;
        print_summary(Some(&path), stage_count);
    } else {
        print_summary(None, 0);
    }

    // Success - disarm the guard to prevent cleanup
    guard.disarm();

    Ok(())
}

/// Resolve the terminal backend choice for `loom init`.
///
/// Precedence: an explicit `--backend` flag always wins (clap's
/// `value_parser` already constrains it to "native"/"tmux"). Otherwise, on
/// an interactive terminal, prompt the operator. Otherwise (programmatic
/// init, non-TTY) default to native so init never hangs.
fn resolve_backend_choice(flag: Option<String>) -> Result<SessionBackendKind> {
    let kind = if let Some(value) = flag {
        match value.as_str() {
            "native" => SessionBackendKind::Native,
            "tmux" => SessionBackendKind::Tmux,
            other => bail!("Invalid terminal backend: {other}"),
        }
    } else if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        prompt_backend_choice()?
    } else {
        SessionBackendKind::Native
    };

    if kind == SessionBackendKind::Tmux && which::which("tmux").is_err() {
        eprintln!(
            "  {} tmux backend selected but tmux was not found on PATH - install tmux \
             before running `loom run`, or re-run `loom init` with `--backend native`",
            "!".yellow().bold()
        );
    }

    Ok(kind)
}

/// Interactively prompt for the terminal backend, re-prompting on invalid input.
fn prompt_backend_choice() -> Result<SessionBackendKind> {
    loop {
        print!("Terminal backend for sessions [native/tmux] (native): ");
        std::io::stdout().flush().ok();

        let mut response = String::new();
        let bytes_read = std::io::stdin().read_line(&mut response)?;
        if bytes_read == 0 {
            // EOF - default to native.
            return Ok(SessionBackendKind::Native);
        }

        match response.trim().to_ascii_lowercase().as_str() {
            "" | "native" => return Ok(SessionBackendKind::Native),
            "tmux" => return Ok(SessionBackendKind::Tmux),
            _ => println!("  Please enter 'native' or 'tmux'."),
        }
    }
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
