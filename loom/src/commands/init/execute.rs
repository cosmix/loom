//! Main execution entry point for loom init command.

use crate::fs::permissions::{ensure_loom_permissions, migrate_legacy_trust};
use crate::fs::work_dir::WorkDir;
use crate::fs::work_integrity::validate_work_dir_state;
use crate::git::install_pre_commit_hook;
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

use super::cleanup::{
    cleanup_orphaned_sessions, cleanup_work_directory, cleanup_worktrees_directory,
    prune_stale_worktrees, remove_work_directory_on_failure,
};
use super::plan_setup::{initialize_with_plan, probe_firewall_or_bail};

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
/// * `backend` - Optional project backend override (`native` | `container`).
/// * `no_build` - When provisioning the container backend, skip the actual
///   image build and pin `image_digest = "pending"`.
/// * `allow_insecure_runtime` - Skip the firewall enforcement smoke test
///   that runs after image build. Use only on runtimes known to lack
///   reliable iptables egress filtering.
pub fn execute(
    plan_path: Option<PathBuf>,
    clean: bool,
    backend: Option<String>,
    no_build: bool,
    allow_insecure_runtime: bool,
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
    cleanup_orphaned_sessions()?;

    if clean {
        cleanup_work_directory(&repo_root)?;
        cleanup_worktrees_directory(&repo_root)?;
    }

    println!("\n{}", "Initialize".bold());
    println!("{}", "─".repeat(40).dimmed());

    // Per finding #11: pre-existing .work must NEVER be deleted on failure.
    // Only arm the cleanup guard when this invocation actually created the
    // directory.
    let work_dir_existed = repo_root.join(".work").exists();
    let mut guard = InitGuard::new(repo_root.clone());
    let work_dir = WorkDir::new(".")?;
    if !work_dir_existed {
        work_dir.initialize()?;
        guard.mark_work_created();
        println!(
            "  {} Directory structure created {}",
            "✓".green().bold(),
            ".work/".dimmed()
        );
    } else {
        work_dir.load()?;
        println!(
            "  {} Reusing existing {} (reconfigure mode)",
            "→".cyan().bold(),
            ".work/".dimmed()
        );
    }

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
        let stage_count = initialize_with_plan(&work_dir, &path)?;
        print_summary(Some(&path), stage_count);
    } else {
        print_summary(None, 0);
    }

    // Per finding #11: project-level backend (`--backend`) is applied AFTER
    // plan setup so a reconfigure invocation can flip the backend without
    // touching stage definitions. When `backend` is None we PRESERVE the
    // existing `[project_execution]` section.
    if let Some(backend_str) = backend {
        apply_project_backend(
            &work_dir,
            &repo_root,
            &backend_str,
            no_build,
            allow_insecure_runtime,
        )?;
    }

    // Success - disarm the guard to prevent cleanup
    guard.disarm();

    Ok(())
}

/// Apply a project-level backend selection to `.work/config.toml`.
///
/// For container backend: detects runtime, computes fingerprint, builds the
/// image (unless `no_build`), and pins the resulting digest. For native
/// backend: clears any container metadata.
fn apply_project_backend(
    work_dir: &WorkDir,
    repo_root: &Path,
    backend_str: &str,
    no_build: bool,
    allow_insecure_runtime: bool,
) -> Result<()> {
    use crate::plan::schema::execution::{
        BackendType, ProjectContainerConfig, ProjectExecutionConfig,
    };

    let backend_type: BackendType = backend_str
        .parse()
        .with_context(|| format!("Invalid --backend value: {backend_str}"))?;

    println!("\n{}", "Backend".bold());
    println!("{}", "─".repeat(40).dimmed());

    match backend_type {
        BackendType::Native => {
            crate::fs::work_dir::write_project_execution(
                work_dir.root(),
                &ProjectExecutionConfig {
                    backend: BackendType::Native,
                    container: None,
                },
            )?;
            println!("  {} Backend: native", "✓".green().bold());
        }
        BackendType::Container => {
            use crate::orchestrator::terminal::container::{
                fingerprint as fp, image, runtime as rt,
            };
            let project_root_for_fp = work_dir
                .project_root()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| repo_root.to_path_buf());
            let runtime = rt::detect_runtime("auto")?;
            let fingerprint = fp::compute_fingerprint(&project_root_for_fp, &[]);
            let started = std::time::Instant::now();
            let (digest, action) = if no_build {
                ("pending".to_string(), "skipped (--no-build)")
            } else {
                let d = image::ensure_image(&fingerprint, runtime, false)?;
                (d, "built")
            };

            // Read existing container config so a reconfigure invocation
            // preserves manually-set or previously-resolved git identity.
            let existing_container = crate::fs::work_dir::read_project_execution(work_dir.root())
                .ok()
                .flatten()
                .and_then(|c| c.container);

            let git_user_name = existing_container
                .as_ref()
                .and_then(|c| c.git_user_name.clone())
                .or_else(|| host_git_config("user.name"))
                .and_then(|v| sanitize_git_identity("user.name", v));
            let git_user_email = existing_container
                .as_ref()
                .and_then(|c| c.git_user_email.clone())
                .or_else(|| host_git_config("user.email"))
                .and_then(|v| sanitize_git_identity("user.email", v));

            if git_user_name.is_none() || git_user_email.is_none() {
                println!(
                    "  {} No git user.name/email on host; commits inside container will use defaults. \
                     Set via: git config --global user.name '...'",
                    "!".yellow().bold()
                );
            }

            crate::fs::work_dir::write_project_execution(
                work_dir.root(),
                &ProjectExecutionConfig {
                    backend: BackendType::Container,
                    container: Some(ProjectContainerConfig {
                        runtime: runtime.binary().to_string(),
                        fingerprint: fingerprint.clone(),
                        image_digest: digest.clone(),
                        // Default to forwarding Claude Code's OAuth
                        // credentials. Container backend's primary use
                        // case is running claude inside an isolated
                        // environment; without these the agent has no
                        // way to authenticate and immediately fails
                        // with "Not logged in · Please run /login".
                        // The credential file is mounted ro at
                        // /home/loom/.claude/.credentials.json — the
                        // agent can use it but cannot mutate it, so
                        // there is no privilege-escalation path.
                        forward_credentials: vec!["claude".to_string()],
                        git_user_name,
                        git_user_email,
                    }),
                },
            )?;
            let elapsed = started.elapsed();
            println!("  {} Backend: container", "✓".green().bold());
            println!("    Runtime:     {}", runtime);
            println!("    Fingerprint: {}", fingerprint);
            println!("    Image:       {} ({})", digest, action);
            println!("    Elapsed:     {:?}", elapsed);

            // Run the firewall enforcement smoke test after the image is
            // available. The probe is skipped when `--no-build` is set
            // (no real image to probe) and when the operator explicitly
            // opts out via `--allow-insecure-runtime`.
            if !no_build && !allow_insecure_runtime {
                let image_ref = format!("loom/base:{fingerprint}");
                probe_firewall_or_bail(runtime, &image_ref)?;
                println!("  {} Firewall enforcement verified", "✓".green().bold());
            } else if allow_insecure_runtime {
                println!(
                    "  {} Firewall smoke test skipped (--allow-insecure-runtime)",
                    "!".yellow().bold()
                );
            }
        }
    }

    Ok(())
}

/// Drop a git identity value that fails [`validate_git_identity`] (control
/// chars, embedded newlines, oversize). Falling back to `None` makes the
/// container path skip env-injection, which is safer than passing the bad
/// value through and getting a malformed commit object or a corrupted env
/// block downstream.
fn sanitize_git_identity(key: &str, value: String) -> Option<String> {
    match crate::plan::schema::execution::validate_git_identity(&value) {
        Ok(()) => Some(value),
        Err(err) => {
            println!(
                "  {} Ignoring invalid git {}: {}",
                "!".yellow().bold(),
                key,
                err,
            );
            None
        }
    }
}

/// Query the host's global git config for a single key.
///
/// Returns `None` when git is unavailable, the key is unset, or the value
/// is empty — all of which mean the container should fall back to git's own
/// defaults rather than injecting a partial identity.
fn host_git_config(key: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["config", "--global", key])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8(out.stdout).ok()?;
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
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
