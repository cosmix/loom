//! `loom knowledge bootstrap`: build a repository's knowledge base.
//!
//! The command scaffolds `doc/loom/knowledge/`, brings the catalog and the
//! source graph current, partitions the repository into directory
//! clusters (`clusters::Cluster`), and briefs one interactive Claude session that
//! writes knowledge through the `loom knowledge` CLI. A session that signals
//! completion leaves a receipt (`receipt::Receipt`), so `--refresh` explores only the
//! clusters that changed since.

mod clusters;
mod graph;
mod prompt;
mod receipt;
#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "tests_clusters.rs"]
mod tests_clusters;
#[cfg(test)]
#[path = "tests_receipt.rs"]
mod tests_receipt;

use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use anyhow::{bail, Context, Result};
use colored::Colorize;

use crate::claude::{remove_if_exists, ClaudeOutcome, ExitAction, AGENT_TEAMS_ENV};
use crate::cli::BootstrapArgs;
use crate::context::graph_store::ResolvedGraph;
use crate::context::store::ContextStore;
use crate::fs::knowledge::{templates, KnowledgeDir, KnowledgeFile};
use crate::git::runner::{run_git, run_git_bool, run_git_checked};
use crate::models::stage::StageType;
use crate::user_config::UserConfig;
use clusters::{Cluster, KNOWLEDGE_PREFIX};
use prompt::PlanSummary;
use receipt::{ClusterStatus, Receipt, RefreshPlan, RECEIPT_FILENAME};

/// Bootstrap's scratch directory: the run lock, the brief and the marker.
const BOOTSTRAP_DIR: &str = ".loom/work/bootstrap";

const STAGE_REFUSAL: &str = "loom knowledge bootstrap is an operator command; inside a stage use \
     loom knowledge update (unset LOOM_STAGE_ID if this shell is not a stage session)";

/// A cluster the session must explore, with its status against the receipt.
struct WorkItem<'a> {
    cluster: &'a Cluster,
    status: ClusterStatus,
}

/// What the steps after planning share.
struct Session<'a> {
    repo_root: &'a Path,
    knowledge_root: &'a Path,
    store: &'a ContextStore,
    /// `ResolvedGraph::base_revision` of the graph the clusters came from.
    source_revision: &'a str,
    coverage: &'a str,
}

/// Entry point for `loom knowledge bootstrap`.
pub(crate) fn execute(args: BootstrapArgs) -> Result<()> {
    guard_not_in_stage()?;
    let repo_root = resolve_repo_root()?;
    ensure_loom_ignored(&repo_root)?;
    let _run_lock = acquire_run_lock(&repo_root)?;

    let knowledge = scaffold(&repo_root)?;
    let (knowledge_root, store) = crate::context::retrieve::resolve_roots(&repo_root)?;
    refresh_derived(&store, &knowledge_root)?;

    let graph = graph::load_current_graph(&repo_root)?;
    let coverage = crate::context::CoverageReport::of(&graph).to_string();
    println!("{coverage}");

    let session = Session {
        repo_root: &repo_root,
        knowledge_root: &knowledge_root,
        store: &store,
        source_revision: &graph.base_revision,
        coverage: &coverage,
    };
    plan_and_run(&session, &knowledge, &graph, &args)
}

/// Partition, compare against the receipt, report, and brief the session
/// unless nothing needs exploring.
fn plan_and_run(
    session: &Session<'_>,
    knowledge: &KnowledgeDir,
    graph: &ResolvedGraph,
    args: &BootstrapArgs,
) -> Result<()> {
    let facts = clusters::file_facts(graph);
    let clusters = clusters::partition(&facts, &clusters::fan_in(graph));
    let gaps = tier1_gaps(knowledge)?;
    let receipt = Receipt::load(session.knowledge_root)?;
    if args.refresh && receipt.is_none() {
        println!("no receipt; running a full bootstrap");
    }
    let plan = receipt::refresh_plan(&clusters, receipt.as_ref());
    let work = work_set(&clusters, &plan, args.refresh);
    let summary = PlanSummary {
        repo_root: session.repo_root,
        source_revision: session.source_revision,
        coverage: session.coverage,
        refresh: args.refresh,
        clusters: &clusters,
        plan: &plan,
        work: &work,
        gaps: &gaps,
    };
    print!("{}", prompt::render_report(&summary));

    if args.structural_only {
        println!("structural-only: no model run");
        return Ok(());
    }
    if work.is_empty() && gaps.is_empty() && plan.removed.is_empty() {
        println!("{}", current_message(receipt.as_ref()));
        return Ok(());
    }
    launch(session, &clusters, args, &prompt::render_brief(&summary))
}

/// Refuse to run inside a stage session. Same test as
/// `commands::hook::target::non_empty_env`, which is private to `hook`.
fn guard_not_in_stage() -> Result<()> {
    let in_stage = std::env::var("LOOM_STAGE_ID").is_ok_and(|value| !value.trim().is_empty());
    if in_stage {
        bail!(STAGE_REFUSAL);
    }
    Ok(())
}

/// `git rev-parse --show-toplevel` from the cwd, with no cwd fallback, and a
/// `HEAD` the source graph can enumerate.
fn resolve_repo_root() -> Result<PathBuf> {
    let output = run_git(&["rev-parse", "--show-toplevel"], Path::new("."))?;
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() || root.is_empty() {
        bail!("loom knowledge bootstrap must run inside a git repository");
    }
    let root = PathBuf::from(root);
    if !run_git_bool(&["rev-parse", "--verify", "--quiet", "HEAD"], &root) {
        bail!("loom knowledge bootstrap needs at least one commit");
    }
    Ok(root)
}

/// Write `.loom/.gitignore` (`*`) when nothing under `.loom/` is tracked, the
/// file does not exist, and the repository does not already ignore both
/// `.loom/cache` and `.loom/work`. Never touches the repository's own
/// `.gitignore`.
fn ensure_loom_ignored(repo_root: &Path) -> Result<()> {
    let ignore_file = repo_root.join(".loom").join(".gitignore");
    let tracked = run_git_checked(&["ls-files", "-z", ".loom"], repo_root)?;
    // symlink_metadata: a dangling symlink counts as present, never written through.
    if !tracked.is_empty() || std::fs::symlink_metadata(&ignore_file).is_ok() {
        return Ok(());
    }
    let ignored = |path: &str| run_git_bool(&["check-ignore", "-q", path], repo_root);
    if ignored(".loom/cache") && ignored(".loom/work") {
        return Ok(());
    }
    std::fs::create_dir_all(repo_root.join(".loom")).context("failed to create .loom/")?;
    std::fs::write(&ignore_file, "*\n")
        .with_context(|| format!("failed to write {}", ignore_file.display()))?;
    println!("wrote .loom/.gitignore so loom's cache and scratch files stay out of git");
    Ok(())
}

/// Take the non-blocking run lock `.loom/work/bootstrap/.lock`; the returned
/// `File` holds it until dropped.
fn acquire_run_lock(repo_root: &Path) -> Result<File> {
    let dir = repo_root.join(BOOTSTRAP_DIR);
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let path = dir.join(".lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed to open run lock {}", path.display()))?;
    match fs2::FileExt::try_lock_exclusive(&file) {
        Ok(()) => Ok(file),
        Err(error) if error.kind() == ErrorKind::WouldBlock => bail!(
            "another loom knowledge bootstrap is already running in {}",
            repo_root.display()
        ),
        Err(error) => {
            Err(error).with_context(|| format!("failed to lock run lock {}", path.display()))
        }
    }
}

/// Create `doc/loom/knowledge/` or fill in its missing tier-1 files. Safe on
/// an existing directory: `initialize` never overwrites a file.
fn scaffold(repo_root: &Path) -> Result<KnowledgeDir> {
    let knowledge = KnowledgeDir::new(repo_root);
    let fresh = !knowledge.exists();
    knowledge
        .initialize()
        .context("failed to scaffold doc/loom/knowledge/")?;
    if fresh {
        println!("scaffolded doc/loom/knowledge/");
    }
    Ok(knowledge)
}

/// `loom knowledge sync`'s sequence, always including the source graph:
/// bootstrap's `--structural-only` means "no model", and the clusters need
/// the graph.
fn refresh_derived(store: &ContextStore, knowledge_root: &Path) -> Result<()> {
    let upgraded = super::sync::upgrade_flat_layout(knowledge_root)?;
    if !upgraded {
        super::sync::refresh_index_best_effort(knowledge_root);
    }
    let outcome = crate::context::refresh::refresh(store, knowledge_root, false)
        .context("failed to rebuild the context catalog and source graph")?;
    super::sync::print_human(&outcome, upgraded);
    Ok(())
}

/// Tier-1 files whose content is still exactly their template.
fn tier1_gaps(knowledge: &KnowledgeDir) -> Result<Vec<KnowledgeFile>> {
    let mut gaps = Vec::new();
    for &file in KnowledgeFile::all() {
        let content = knowledge.read(file)?;
        if content.trim() == templates::default_content(file).trim() {
            gaps.push(file);
        }
    }
    Ok(gaps)
}

/// Every cluster, or with `refresh` only the new and changed ones.
/// `plan.statuses` is in `clusters` order (`receipt::refresh_plan`).
fn work_set<'a>(clusters: &'a [Cluster], plan: &RefreshPlan, refresh: bool) -> Vec<WorkItem<'a>> {
    clusters
        .iter()
        .zip(&plan.statuses)
        .map(|(cluster, (_, status))| WorkItem {
            cluster,
            status: *status,
        })
        .filter(|item| !refresh || item.status != ClusterStatus::Unchanged)
        .collect()
}

fn current_message(receipt: Option<&Receipt>) -> String {
    match receipt {
        Some(receipt) => format!(
            "knowledge is current (receipt {}, revision {})",
            receipt.completed_at, receipt.source_revision
        ),
        None => "knowledge is current".to_string(),
    }
}

/// Write the brief, then print the command (`--dry-run`) or run the session.
fn launch(
    session: &Session<'_>,
    clusters: &[Cluster],
    args: &BootstrapArgs,
    brief_text: &str,
) -> Result<()> {
    let dir = session.repo_root.join(BOOTSTRAP_DIR);
    let pid = std::process::id();
    let brief = dir.join(format!("brief-{pid}.md"));
    let marker = dir.join(format!("claude-{pid}.done"));
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    std::fs::write(&brief, brief_text)
        .with_context(|| format!("failed to write brief {}", brief.display()))?;

    let (model, effort) = resolve_model_effort(args);
    let argv = prompt::claude_args(&brief, &marker, &model, &effort);
    if args.dry_run {
        println!("brief: {}", brief.display());
        println!("{AGENT_TEAMS_ENV}=1 claude {}", argv.join(" "));
        return Ok(());
    }

    let outcome = spawn_session(session.repo_root, &argv, &marker);
    remove_brief(&brief);
    let outcome = outcome?;
    finalize(session);
    conclude(outcome, session, clusters, &model, &effort)
}

/// The `--model`/`--effort` flags, else the knowledge stage defaults.
fn resolve_model_effort(args: &BootstrapArgs) -> (String, String) {
    let user = UserConfig::load();
    let model = args
        .model
        .clone()
        .unwrap_or_else(|| user.stage_model(StageType::Knowledge).to_string());
    let effort = args.effort.clone().unwrap_or_else(|| {
        user.stage_reasoning_effort(StageType::Knowledge)
            .to_string()
    });
    (model, effort)
}

fn spawn_session(repo_root: &Path, argv: &[String], marker: &Path) -> Result<ClaudeOutcome> {
    let claude_path = crate::claude::find_claude_path()
        .context("claude not found; install Claude Code or use --structural-only")?;
    crate::claude::run_foreground(&claude_path, repo_root, argv, marker)
}

fn remove_brief(brief: &Path) {
    if let Err(error) = remove_if_exists(brief) {
        eprintln!("warning: {error:#}");
    }
}

/// Pick up the session's writes. Best-effort: the session already ran, and
/// a stale cache must not cost it its receipt.
fn finalize(session: &Session<'_>) {
    if let Err(error) = refresh_derived(session.store, session.knowledge_root) {
        eprintln!("warning: refresh after the session failed: {error:#}");
    }
    match crate::fs::knowledge::catalog::build(session.knowledge_root) {
        Ok(catalog) => println!(
            "knowledge issues: {} (run loom knowledge check for details)",
            catalog.issues.len()
        ),
        Err(error) => eprintln!("warning: failed to build the knowledge catalog: {error:#}"),
    }
}

/// Write the receipt only when the session touched its marker.
fn conclude(
    outcome: ClaudeOutcome,
    session: &Session<'_>,
    clusters: &[Cluster],
    model: &str,
    effort: &str,
) -> Result<()> {
    let status = match outcome {
        ClaudeOutcome::Completed => {
            Receipt::from_clusters(clusters, session.source_revision, model, effort)
                .save(session.knowledge_root)?;
            println!(
                "receipt: {KNOWLEDGE_PREFIX}{RECEIPT_FILENAME} (commit it with the knowledge)"
            );
            return Ok(());
        }
        ClaudeOutcome::Exited(status) => status,
    };
    match crate::claude::classify_exit(status) {
        ExitAction::Continue => {
            println!(
                "{} session ended without signalling completion; receipt not written",
                "!".yellow().bold()
            );
            Ok(())
        }
        ExitAction::Abort | ExitAction::Warn => bail!(
            "claude exited with {}; receipt not written",
            describe_exit(status)
        ),
    }
}

fn describe_exit(status: ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("code {code}"),
        (None, Some(signal)) => format!("signal {signal}"),
        (None, None) => status.to_string(),
    }
}
