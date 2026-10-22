//! `StageQuery::dependency_paths` over real git repositories: the change set
//! a dependency's merge brought to the target, and the declared-file fallback.

use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tempfile::TempDir;

use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
use crate::context::schema::{FileCoverage, NodeLanguage, SourceNode, SourceNodeKind, Span};
use crate::context::store::ContextStore;
use crate::git::merge::{merge_stage, MergeGate};
use crate::models::session::Session;
use crate::orchestrator::signals::tests::{create_test_session, create_test_worktree};
use crate::orchestrator::signals::{generate_contract_signal, generate_signal};
use crate::verify::transitions::save_stage;

/// Run git in `root` with the global and system config isolated.
fn git(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository on `main` with one commit and `.loom/work/stages` in place.
fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    let exclude = root.join(".git/info/exclude");
    let mut text = std::fs::read_to_string(&exclude).unwrap_or_default();
    text.push_str(".loom/\n");
    std::fs::write(exclude, text).unwrap();
    std::fs::write(root.join("seed.txt"), "seed").unwrap();
    git(root, &["add", "seed.txt"]);
    git(root, &["commit", "-m", "seed"]);
    std::fs::create_dir_all(root.join(".loom/work/stages")).unwrap();
    temp
}

/// Create `loom/up` from `main` changing `src/up.rs`, return to `main`, and
/// return the branch tip.
fn upstream_branch(root: &Path) -> String {
    git(root, &["checkout", "-b", "loom/up", "main"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/up.rs"), "pub fn upstream_widget() {}\n").unwrap();
    git(root, &["add", "src/up.rs"]);
    git(root, &["commit", "-m", "up"]);
    let tip = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "main"]);
    tip
}

fn merge_upstream(root: &Path) {
    let lock = TempDir::new().unwrap();
    merge_stage("up", "main", root, lock.path(), MergeGate::Enforce).unwrap();
}

/// Save a Completed `up` record at `tip` declaring `files`.
fn save_upstream(root: &Path, tip: Option<&str>, files: &[&str], edit: impl FnOnce(&mut Stage)) {
    let mut up = Stage::new("Upstream".to_string(), None);
    up.id = "up".to_string();
    up.status = StageStatus::Completed;
    up.completed_commit = tip.map(str::to_string);
    up.files = files.iter().map(|f| f.to_string()).collect();
    edit(&mut up);
    save_stage(&up, &root.join(".loom/work")).unwrap();
}

fn downstream() -> Stage {
    let mut down = Stage::new(
        "Downstream".to_string(),
        Some("Extend UpstreamWidget".to_string()),
    );
    down.id = "down".to_string();
    down.dependencies = vec!["up".to_string()];
    down
}

fn paths(root: &Path) -> Vec<String> {
    StageQuery::dependency_paths(&root.join(".loom/work"), &downstream())
}

/// A repository whose `up` stage changed `src/up.rs`, was merged, and is saved
/// as accepted with the glob `src/**` as its only declared file.
fn merged_fixture() -> TempDir {
    let temp = init_repo();
    let root = temp.path();
    let tip = upstream_branch(root);
    merge_upstream(root);
    save_upstream(root, Some(&tip), &["src/**"], |up| up.merged = true);
    temp
}

#[test]
fn a_merged_dependency_contributes_its_changed_files() {
    let temp = merged_fixture();
    assert_eq!(paths(temp.path()), vec!["src/up.rs".to_string()]);
}

#[test]
fn an_unmerged_dependency_used_as_base_contributes_its_changed_files() {
    let temp = init_repo();
    let tip = upstream_branch(temp.path());
    save_upstream(temp.path(), Some(&tip), &["src/**"], |_| {});
    assert_eq!(paths(temp.path()), vec!["src/up.rs".to_string()]);
}

#[test]
fn an_assumed_merge_without_a_merge_commit_falls_back_to_declared_files() {
    let temp = init_repo();
    let tip = upstream_branch(temp.path());
    save_upstream(temp.path(), Some(&tip), &["src/declared.rs"], |up| {
        up.merge_assumed = true
    });
    assert_eq!(paths(temp.path()), vec!["src/declared.rs".to_string()]);
}

#[test]
fn a_dependency_without_a_commit_joins_declared_files_to_its_working_dir() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join(".loom/work/stages")).unwrap();
    let files = ["src/a.rs", "src/**/*.rs", "../web/b.ts"];
    save_upstream(temp.path(), None, &files, |up| {
        up.working_dir = Some("loom".to_string());
        up.artifacts = vec!["src/a.rs".to_string()];
    });
    assert_eq!(
        paths(temp.path()),
        vec!["loom/src/a.rs".to_string(), "web/b.ts".to_string()]
    );
}

#[test]
fn a_knowledge_dependency_uses_declared_files() {
    let temp = init_repo();
    let tip = upstream_branch(temp.path());
    merge_upstream(temp.path());
    save_upstream(
        temp.path(),
        Some(&tip),
        &["doc/loom/knowledge/x.md"],
        |up| {
            up.stage_type = StageType::Knowledge;
            up.merged = true;
        },
    );
    assert_eq!(
        paths(temp.path()),
        vec!["doc/loom/knowledge/x.md".to_string()]
    );
}

#[test]
fn an_ancestor_without_a_merge_commit_falls_back_to_declared_files() {
    let temp = init_repo();
    let root = temp.path();
    let tip = upstream_branch(root);
    git(root, &["merge", "--ff-only", "loom/up"]);
    save_upstream(root, Some(&tip), &["src/declared.rs"], |up| {
        up.merged = true
    });
    assert_eq!(paths(root), vec!["src/declared.rs".to_string()]);
}

/// A source node `src/up.rs#function:UpstreamWidget`, in the overlay the
/// `down` stage's brief reads (`("default", "down")`: the stage names no plan).
fn write_stage_overlay(root: &Path) {
    let node = SourceNode {
        id: "src/up.rs#function:UpstreamWidget".to_string(),
        kind: SourceNodeKind::Function,
        path: PathBuf::from("src/up.rs"),
        scope: vec!["UpstreamWidget".to_string()],
        span: Span {
            start_byte: 0,
            end_byte: 40,
            line_start: 1,
            line_end: 2,
        },
        signature: "pub fn upstream_widget()".to_string(),
        body_hash: "sha256:upstream-widget".to_string(),
        language: NodeLanguage::Rust,
        parser_version: "test+v1".to_string(),
        coverage: FileCoverage::Full,
        symbol_key: String::new(),
    };
    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    let mut files = BTreeMap::new();
    files.insert(
        "src/up.rs".to_string(),
        FileEntry {
            content_hash: "sha256:up-file".to_string(),
            nodes: vec![node],
            edges: Vec::new(),
            coverage: FileCoverage::Full,
            imports: Vec::new(),
        },
    );
    let layer = GraphLayer {
        revision: "test-revision".to_string(),
        generation: String::new(),
        built_at: None,
        files,
        blob_index: BTreeMap::new(),
        schema_version: crate::context::source_graph::GRAPH_SCHEMA_VERSION,
    };
    GraphStore::new(store.root(), work_dir.root())
        .save_overlay("default", "down", &layer)
        .unwrap();
}

/// The signal's source bullet for `src/up.rs` must name the symbol and the
/// dependency boost; under the declared glob `src/**` nothing matched.
fn assert_boosted_bullet(signal: &str) {
    let line = signal
        .lines()
        .find(|line| line.contains("`src/up.rs`"))
        .unwrap_or_else(|| panic!("no source bullet for src/up.rs: {signal}"));
    assert!(line.contains("UpstreamWidget"), "{line}");
    assert!(line.contains("stage-dependency"), "{line}");
}

#[test]
fn the_contract_signal_boosts_a_symbol_from_the_dependency_change() {
    let temp = merged_fixture();
    let root = temp.path();
    write_stage_overlay(root);
    let work_dir = root.join(".loom/work");

    let path = generate_contract_signal(
        &Session::new_contract("down"),
        &downstream(),
        &create_test_worktree(),
        &work_dir,
        None,
        &[],
    )
    .unwrap();
    assert_boosted_bullet(&std::fs::read_to_string(path).unwrap());
}

#[test]
fn the_stage_brief_boosts_a_symbol_from_the_dependency_change() {
    let temp = merged_fixture();
    let root = temp.path();
    write_stage_overlay(root);
    let work_dir = root.join(".loom/work");

    let path = generate_signal(
        &create_test_session(),
        &downstream(),
        &create_test_worktree(),
        &[],
        None,
        None,
        &work_dir,
    )
    .unwrap();
    assert_boosted_bullet(&std::fs::read_to_string(path).unwrap());
}
