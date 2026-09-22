use std::path::Path;
use std::process::Command;
use std::time::Duration;

use clap::Parser;
use tempfile::TempDir;

use super::clusters::Cluster;
use super::graph::require_snapshot;
use super::prompt::{self, PlanSummary};
use super::receipt::{ClusterStatus, RefreshPlan};
use super::{
    acquire_run_lock, ensure_loom_ignored, resolve_model_effort, scaffold, tier1_gaps, work_set,
    WorkItem,
};
use crate::cli::BootstrapArgs;
use crate::context::refresh::{SnapshotAction, SnapshotOutcome, SourceGraphCounters};
use crate::fs::knowledge::templates::default_content;
use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile, KnowledgeTarget};
use crate::user_config::parse_document;

fn cluster(id: &str, files: usize) -> Cluster {
    Cluster {
        id: id.to_string(),
        files: (0..files).map(|i| format!("{id}/f{i}.rs")).collect(),
        symbols: files,
        digest: format!("sha256:{id}"),
        hot_files: Vec::new(),
    }
}

fn refresh_plan(statuses: &[(&str, ClusterStatus)], removed: &[&str]) -> RefreshPlan {
    RefreshPlan {
        statuses: statuses
            .iter()
            .map(|(id, status)| (id.to_string(), *status))
            .collect(),
        removed: removed.iter().map(|id| id.to_string()).collect(),
    }
}

fn summary<'a>(
    clusters: &'a [Cluster],
    plan: &'a RefreshPlan,
    work: &'a [WorkItem<'a>],
    gaps: &'a [KnowledgeFile],
    refresh: bool,
) -> PlanSummary<'a> {
    PlanSummary {
        repo_root: Path::new("/repo"),
        source_revision: "abc123",
        coverage: "coverage: test",
        refresh,
        clusters,
        plan,
        work,
        gaps,
    }
}

fn ids(items: &[WorkItem<'_>]) -> Vec<String> {
    items.iter().map(|item| item.cluster.id.clone()).collect()
}

#[test]
fn claude_args_keep_disallowed_tools_before_the_prompt() {
    let brief = Path::new("/repo/.loom/work/bootstrap/brief-1.md");
    let marker = Path::new("/repo/.loom/work/bootstrap/claude-1.done");
    let args = prompt::claude_args(brief, marker, "sonnet", "low");

    let tools = args.iter().position(|a| a == "--disallowedTools").unwrap();
    assert_eq!(args[tools + 1], "Edit,Write,NotebookEdit");
    let append = args
        .iter()
        .position(|a| a == "--append-system-prompt")
        .unwrap();
    assert!(append > tools + 1);
    assert_eq!(args.last(), Some(&prompt::initial_prompt(brief)));
    assert!(args.last().unwrap().contains("brief-1.md"));
    // Interactive session: no flag forces print/non-interactive mode.
    assert!(!args.iter().any(|a| a == "-p" || a == "--print"));
}

#[test]
fn system_prompt_names_the_marker_touch() {
    let marker = Path::new("/repo/.loom/work/bootstrap/claude-7.done");
    let text = prompt::system_prompt(marker);
    assert!(text.contains("touch /repo/.loom/work/bootstrap/claude-7.done"));
    assert!(text.contains("Write knowledge ONLY with `loom knowledge update`"));
}

#[test]
fn render_brief_lists_work_removed_and_gaps_with_refresh_statuses() {
    use ClusterStatus::{Changed, New, Unchanged};
    let clusters = [cluster("a", 1), cluster("b", 3), cluster("c", 2)];
    let plan = refresh_plan(&[("a", Unchanged), ("b", Changed), ("c", New)], &["gone"]);
    let gaps = [KnowledgeFile::Stack];

    let work = work_set(&clusters, &plan, true);
    let brief = prompt::render_brief(&summary(&clusters, &plan, &work, &gaps, true));
    assert!(brief.contains("| `b` | 3 | 3 | changed | - |"), "{brief}");
    assert!(brief.contains("| `c` | 2 | 2 | new | - |"), "{brief}");
    assert!(!brief.contains("| `a` |"), "{brief}");
    assert!(brief.contains("- removed: gone"), "{brief}");
    assert!(brief.contains("- `doc/loom/knowledge/stack.md`"), "{brief}");

    let work = work_set(&clusters, &plan, false);
    let brief = prompt::render_brief(&summary(&clusters, &plan, &work, &gaps, false));
    assert!(brief.contains("| `a` | 1 | 1 | - |"), "{brief}");
    assert!(brief.contains("| `b` | 3 | 3 | - |"), "{brief}");
    assert!(brief.contains("| `c` | 2 | 2 | - |"), "{brief}");
    for status in ["new", "changed", "unchanged"] {
        assert!(!brief.contains(&format!("| {status} |")), "{brief}");
    }
}

fn git_repo(gitignore: Option<&str>) -> TempDir {
    let dir = TempDir::new().unwrap();
    // Repo-local excludes beat a developer's global excludes file.
    for args in [
        &["init", "-q"][..],
        &["config", "core.excludesFile", "/dev/null"][..],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
    }
    if let Some(content) = gitignore {
        std::fs::write(dir.path().join(".gitignore"), content).unwrap();
    }
    dir
}

#[test]
fn ensure_loom_ignored_writes_in_a_fresh_repo() {
    let repo = git_repo(None);
    ensure_loom_ignored(repo.path()).unwrap();
    let written = std::fs::read_to_string(repo.path().join(".loom/.gitignore")).unwrap();
    assert_eq!(written, "*\n");
}

#[test]
fn ensure_loom_ignored_skips_when_the_repo_ignores_loom() {
    let repo = git_repo(Some(".loom/\n"));
    ensure_loom_ignored(repo.path()).unwrap();
    assert!(!repo.path().join(".loom/.gitignore").exists());
}

#[test]
fn ensure_loom_ignored_writes_when_only_work_is_ignored() {
    let repo = git_repo(Some(".loom/work/\n"));
    ensure_loom_ignored(repo.path()).unwrap();
    assert!(repo.path().join(".loom/.gitignore").exists());
}

#[test]
fn ensure_loom_ignored_refuses_a_symlinked_loom_dir() {
    let repo = git_repo(None);
    let target = TempDir::new().unwrap();
    std::os::unix::fs::symlink(target.path(), repo.path().join(".loom")).unwrap();

    let error = ensure_loom_ignored(repo.path()).unwrap_err().to_string();
    assert!(error.contains("is a symlink"), "{error}");
    assert!(!target.path().join(".gitignore").exists());
}

#[test]
fn tier1_gaps_count_template_only_files() {
    let temp = TempDir::new().unwrap();
    let knowledge = KnowledgeDir::new(temp.path());
    knowledge.initialize().unwrap();
    assert_eq!(tier1_gaps(&knowledge).unwrap().len(), 7);

    // The code path `loom knowledge update stack "..."` takes.
    let target = KnowledgeTarget::parse("stack").unwrap();
    knowledge
        .append_target(&target, "## Build\n\nCargo workspace.")
        .unwrap();
    let gaps = tier1_gaps(&knowledge).unwrap();
    assert_eq!(gaps.len(), 6);
    assert!(!gaps.contains(&KnowledgeFile::Stack));
}

#[test]
fn scaffold_keeps_a_human_file_and_fills_the_rest() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("doc/loom/knowledge");
    std::fs::create_dir_all(&root).unwrap();
    let human = "# Architecture\n\nWritten by hand.\n";
    std::fs::write(root.join("architecture.md"), human).unwrap();

    let knowledge = scaffold(temp.path()).unwrap();

    let architecture = std::fs::read_to_string(root.join("architecture.md")).unwrap();
    assert_eq!(architecture, human);
    for &file in KnowledgeFile::all() {
        if file != KnowledgeFile::Architecture {
            assert_eq!(knowledge.read(file).unwrap(), default_content(file));
        }
    }
    assert_eq!(tier1_gaps(&knowledge).unwrap().len(), 6);
}

fn snapshot(action: SnapshotAction) -> SnapshotOutcome {
    SnapshotOutcome {
        action,
        reason: "context cache is read-only".to_string(),
        revision: String::new(),
        generation: String::new(),
        overlay: None,
        counters: SourceGraphCounters::default(),
        elapsed: Duration::ZERO,
    }
}

#[test]
fn require_snapshot_refuses_an_unavailable_graph() {
    let error = require_snapshot(&snapshot(SnapshotAction::Unavailable))
        .unwrap_err()
        .to_string();
    assert!(error.contains("context cache is read-only"), "{error}");
    assert!(error.contains("no receipt was written"), "{error}");
    require_snapshot(&snapshot(SnapshotAction::Reused)).unwrap();
}

#[test]
fn run_lock_is_exclusive_until_dropped() {
    let temp = TempDir::new().unwrap();
    let first = acquire_run_lock(temp.path()).unwrap();
    let error = acquire_run_lock(temp.path()).unwrap_err().to_string();
    assert!(error.contains("already running"), "{error}");
    drop(first);
    acquire_run_lock(temp.path()).unwrap();
}

#[test]
fn work_set_drops_unchanged_clusters_only_on_refresh() {
    use ClusterStatus::{Changed, New, Unchanged};
    let clusters = [cluster("a", 1), cluster("b", 1), cluster("c", 1)];
    let plan = refresh_plan(&[("a", New), ("b", Unchanged), ("c", Changed)], &[]);
    assert_eq!(ids(&work_set(&clusters, &plan, true)), ["a", "c"]);
    assert_eq!(ids(&work_set(&clusters, &plan, false)), ["a", "b", "c"]);
}

#[test]
fn structural_only_conflicts_with_model() {
    let parses = |extra: &[&str]| {
        let base = ["loom", "knowledge", "bootstrap", "--structural-only"];
        let args = base.iter().chain(extra).copied();
        crate::cli::Cli::try_parse_from(args).is_ok()
    };
    assert!(!parses(&["--model", "opus"]));
    assert!(parses(&["--refresh"]));
}

fn bootstrap_args(model: Option<&str>, effort: Option<&str>) -> BootstrapArgs {
    BootstrapArgs {
        structural_only: false,
        refresh: false,
        dry_run: false,
        model: model.map(str::to_string),
        effort: effort.map(str::to_string),
    }
}

#[test]
fn resolve_model_effort_falls_back_to_the_config_stage_defaults() {
    let args = bootstrap_args(None, None);
    let user =
        parse_document("[models]\nknowledge_model = \"fable\"\nknowledge_effort = \"low\"\n")
            .unwrap();

    assert_eq!(
        resolve_model_effort(&args, &user),
        ("fable".to_string(), "low".to_string())
    );
}

#[test]
fn resolve_model_effort_prefers_explicit_flags_over_the_config() {
    let args = bootstrap_args(Some("sonnet"), Some("high"));
    let user =
        parse_document("[models]\nknowledge_model = \"fable\"\nknowledge_effort = \"low\"\n")
            .unwrap();

    assert_eq!(
        resolve_model_effort(&args, &user),
        ("sonnet".to_string(), "high".to_string())
    );
}
