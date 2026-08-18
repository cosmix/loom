//! Pre-run checks for loom orchestration
//!
//! Contains validation functions that must pass before starting orchestration.

use anyhow::{bail, Result};
use colored::Colorize;
use std::path::Path;

use crate::context::graph_store::GraphStore;
use crate::context::local_overlay::local_overlay_key;
use crate::context::refresh::{reconcile_source_graph, SourceGraphScope, SOURCE_GRAPH_PREFIX};
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::git::runner::run_git_checked;
use crate::git::{get_uncommitted_changes_summary, has_uncommitted_changes};

/// Ensure the repository is ready for Loom's git worktree operations.
pub fn prepare_repo_for_run(repo_root: &Path) -> Result<()> {
    let repo_bootstrap = crate::git::ensure_repo_ready_for_worktrees(repo_root)?;
    print_repo_bootstrap(repo_bootstrap);

    // Stale per-session identity in the MAIN repo's settings env shadows the
    // wrapper exports of every session this run will spawn (Claude Code
    // applies main-repo settings env to worktree sessions too) — heal before
    // spawning anything.
    for path in crate::fs::permissions::scrub_main_repo_settings_identity(repo_root) {
        println!(
            "{} Removed stale session identity env from {}",
            "✓".green().bold(),
            path.display()
        );
    }

    check_for_uncommitted_changes(repo_root)
}

/// Advisory Codex lane preflight — never aborts startup.
///
/// When any stage licenses the codex lane but the codex CLI or its plugin's
/// companion runtime is missing on this machine, print ONE warning naming the
/// stages. The stage signals independently route codex-tier work to sonnet
/// (the fallback branch of `format_codex_implementers_section`), so this is
/// notice, not enforcement.
pub fn advisory_codex_lane_preflight(repo_root: &Path) {
    let Ok(stages) = crate::verify::transitions::list_all_stages(repo_root) else {
        return;
    };
    let codex_stage_ids: Vec<&str> = stages
        .iter()
        .filter(|s| s.implementers.includes_codex())
        .map(|s| s.id.as_str())
        .collect();
    if codex_stage_ids.is_empty() {
        return;
    }
    if let Err(reason) = crate::codex::codex_lane_status() {
        eprintln!(
            "codex lane licensed for stage(s) {} but unavailable ({reason}) - \
             terra/luna-tier work will fall back to sonnet.",
            codex_stage_ids.join(", ")
        );
        return;
    }
    // Lane installed — but on Linux codex's own workspace-write sandbox must
    // exclude /tmp: it masks `.git` under every writable root, and inside the
    // stage sandbox (read-only /tmp) bwrap cannot create the missing
    // /tmp/.git mountpoint, so every forward dies before the model runs a
    // single command.
    if cfg!(target_os = "linux") {
        if let Some(config_path) = crate::codex::codex_config_path() {
            if !crate::codex::codex_config_excludes_slash_tmp(&config_path) {
                eprintln!(
                    "codex lane licensed for stage(s) {} but ~/.codex/config.toml does not set \
                     sandbox_workspace_write.exclude_slash_tmp - inside the stage sandbox every \
                     codex exec fails with `bwrap: Can't mkdir /tmp/.git: Read-only file system`. \
                     Run `loom repair --fix` to set it.",
                    codex_stage_ids.join(", ")
                );
            }
        }
    }
}

/// Advisory source-graph preflight - never aborts startup.
///
/// Publishes the immutable base source-graph layer for HEAD when none exists
/// yet, so the graph is there before the first stage session is ever briefed
/// rather than only after the first merge. SILENT on the common path (a base
/// for HEAD already exists); every failure degrades to one advisory line.
///
/// `allow_overlay_fallback` decides what happens when the base publish is
/// refused because the tracked tree is dirty. Both `loom run` paths pass
/// false: they have already bailed on a dirty tree, and a run needs a base.
/// `loom init` passes true - it is commonly the first command run in a dirty
/// checkout, where publishing nothing would leave it with no graph at all - and
/// falls back to the working-tree overlay at the address `local_overlay_key`
/// owns, which is the same address retrieval reads by default.
///
/// The [`SOURCE_GRAPH_PREFIX`] on every advisory line here is a convention
/// introduced with this function (shared with `loom knowledge sync`), because
/// this codebase had no shared advisory marker: `advisory_codex_lane_preflight`
/// prints bare text and `foreground.rs` uses a literal "Warning: ".
pub fn advisory_source_graph_preflight(
    repo_root: &Path,
    work_dir: &WorkDir,
    allow_overlay_fallback: bool,
) {
    if let Err(error) = publish_source_graph(repo_root, work_dir, allow_overlay_fallback) {
        eprintln!("{SOURCE_GRAPH_PREFIX}not published ({error:#})");
    }
}

/// The fallible half of [`advisory_source_graph_preflight`]. Every error is
/// swallowed by the caller; nothing here may abort startup.
fn publish_source_graph(
    repo_root: &Path,
    work_dir: &WorkDir,
    allow_overlay_fallback: bool,
) -> Result<()> {
    let store = ContextStore::open(work_dir)?;
    // Idempotent, and required: without it the very first publish on a fresh
    // checkout fails on a missing cache directory.
    store.ensure()?;
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let head = run_git_checked(&["rev-parse", "HEAD"], repo_root)?;

    if graph_store.load_base(&head)?.is_some() {
        return Ok(());
    }

    let base = reconcile_source_graph(
        &store,
        &graph_store,
        repo_root,
        SourceGraphScope::Base {
            revision: head.clone(),
        },
    )?;
    // `reconcile_source_graph` degrades rather than erroring: a refusal comes
    // back as a stale, zero-count freshness carrying the reason.
    if !base.freshness.stale {
        println!(
            "{} Source graph published for {}",
            "✓".green().bold(),
            head.get(..8).unwrap_or(&head)
        );
        return Ok(());
    }

    let refusal = base
        .freshness
        .detail
        .unwrap_or_else(|| "unknown reason".to_string());
    if !allow_overlay_fallback {
        eprintln!("{SOURCE_GRAPH_PREFIX}base not published - {refusal}");
        return Ok(());
    }

    publish_source_graph_overlay(&store, &graph_store, repo_root, work_dir, &refusal)
}

fn publish_source_graph_overlay(
    store: &ContextStore,
    graph_store: &GraphStore,
    repo_root: &Path,
    work_dir: &WorkDir,
    refusal: &str,
) -> Result<()> {
    // The overlay address is keyed on the project root's directory name, and
    // `local_overlay_stage_name` canonicalizes before taking it, so the
    // address identifies the directory rather than how the path happened to
    // be spelled: an absolute repo_root and a relative "." naming the same
    // directory now key to the SAME address. Deriving it from
    // `work_dir.project_root()` - the same call every reader (`loom map`,
    // `loom knowledge sync`/`context`) makes to build the address it reads -
    // is still correct. Do not "simplify" this back to `repo_root`.
    let project_root = work_dir.project_root().unwrap_or(repo_root);
    let (plan, stage) = local_overlay_key(project_root);
    let overlay = reconcile_source_graph(
        store,
        graph_store,
        repo_root,
        SourceGraphScope::Overlay {
            plan: plan.clone(),
            stage: stage.clone(),
        },
    )?;
    println!(
        "{} Source graph written to working-tree overlay {plan}/{stage} ({} files, {} nodes)",
        "✓".green().bold(),
        overlay.files_extracted,
        overlay.nodes
    );
    eprintln!("{SOURCE_GRAPH_PREFIX}base not published - {refusal}");
    Ok(())
}

fn print_repo_bootstrap(result: crate::git::RepoBootstrapResult) {
    if !result.changed() {
        return;
    }

    if result.initialized_repo {
        println!("{} Initialized git repository", "✓".green().bold());
    }

    if result.created_initial_commit {
        println!(
            "{} Created bootstrap commit for worktree support",
            "✓".green().bold()
        );
    }
}

/// Check for uncommitted changes and bail if found
///
/// This prevents starting orchestration with a dirty repository, which could
/// cause issues with worktree creation and branch management.
pub fn check_for_uncommitted_changes(repo_root: &Path) -> Result<()> {
    if has_uncommitted_changes(repo_root)? {
        let summary = get_uncommitted_changes_summary(repo_root)?;
        eprintln!(
            "{} Cannot start loom run with uncommitted changes",
            "✗".red().bold()
        );
        eprintln!();
        if !summary.is_empty() {
            for line in summary.lines() {
                eprintln!("  {}", line.dimmed());
            }
            eprintln!();
        }
        eprintln!("  {} Commit or stash your changes first:", "→".dimmed());
        eprintln!(
            "    {}  Commit changes",
            "git commit -am \"message\"".cyan()
        );
        eprintln!("    {}  Or stash them", "git stash".cyan());
        bail!("Uncommitted changes in repository - commit or stash before running loom");
    }
    Ok(())
}
