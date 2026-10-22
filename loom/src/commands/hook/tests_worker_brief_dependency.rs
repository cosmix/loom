//! A worker brief boosts the files a merged dependency changed.

use super::super::issue;
use super::support::{payload, Fixture, PLAN, STAGE};
use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
use crate::context::schema::{FileCoverage, NodeLanguage, SourceNode, SourceNodeKind, Span};
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::git::merge::{merge_stage, MergeGate};
use crate::models::stage::{Stage, StageStatus};
use crate::verify::transitions::save_stage;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

/// Make `root` a repository on `main` and merge `loom/upstream`, which
/// changes `src/scoped_dep.rs`; return the branch tip.
fn merged_upstream(root: &Path) -> String {
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    let exclude = root.join(".git/info/exclude");
    std::fs::write(exclude, ".loom/\n").unwrap();
    std::fs::write(root.join("seed.txt"), "seed").unwrap();
    git(root, &["add", "seed.txt"]);
    git(root, &["commit", "-m", "seed"]);
    git(root, &["checkout", "-b", "loom/upstream", "main"]);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/scoped_dep.rs"),
        "pub fn dependency_widget() {}\n",
    )
    .unwrap();
    git(root, &["add", "src/scoped_dep.rs"]);
    git(root, &["commit", "-m", "upstream"]);
    let tip = git(root, &["rev-parse", "HEAD"]);
    git(root, &["checkout", "main"]);
    let lock = tempfile::tempdir().unwrap();
    merge_stage("upstream", "main", root, lock.path(), MergeGate::Enforce).unwrap();
    tip
}

/// Write the `DependencyWidget` node into the overlay the worker stage reads.
fn write_dependency_overlay(root: &Path) {
    let node = SourceNode {
        id: "src/scoped_dep.rs#function:DependencyWidget".to_string(),
        kind: SourceNodeKind::Function,
        path: PathBuf::from("src/scoped_dep.rs"),
        scope: vec!["DependencyWidget".to_string()],
        span: Span {
            start_byte: 0,
            end_byte: 40,
            line_start: 1,
            line_end: 2,
        },
        signature: "pub fn DependencyWidget()".to_string(),
        body_hash: "sha256:DependencyWidget".to_string(),
        language: NodeLanguage::Rust,
        parser_version: "test+v1".to_string(),
        coverage: FileCoverage::Full,
        symbol_key: String::new(),
    };
    let work_dir = WorkDir::new(root).unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    let mut files = BTreeMap::new();
    files.insert(
        "src/scoped_dep.rs".to_string(),
        FileEntry {
            content_hash: "sha256:file-DependencyWidget".to_string(),
            nodes: vec![node],
            edges: Vec::new(),
            coverage: FileCoverage::Full,
            imports: Vec::new(),
        },
    );
    let layer = GraphLayer {
        revision: "worker-overlay".to_string(),
        generation: String::new(),
        built_at: None,
        files,
        blob_index: BTreeMap::new(),
        schema_version: crate::context::source_graph::GRAPH_SCHEMA_VERSION,
    };
    GraphStore::new(store.root(), work_dir.root())
        .save_overlay(PLAN, STAGE, &layer)
        .unwrap();
}

#[test]
fn a_worker_brief_boosts_the_dependency_change() {
    let mut fixture = Fixture::new(false);
    let root = fixture._temp.path().to_path_buf();
    std::fs::create_dir_all(fixture.config.work_dir.join("stages")).unwrap();
    let tip = merged_upstream(&root);

    let mut upstream = Stage::new("Upstream".to_string(), None);
    upstream.id = "upstream".to_string();
    upstream.status = StageStatus::Completed;
    upstream.merged = true;
    upstream.completed_commit = Some(tip);
    upstream.files = vec!["src/**".to_string()];
    save_stage(&upstream, &fixture.config.work_dir).unwrap();
    fixture.config.stage.dependencies = vec!["upstream".to_string()];
    write_dependency_overlay(&root);

    let envelope = issue(&payload("Extend DependencyWidget."), &fixture.config)
        .expect("the dependency's changed symbol should be briefed");

    let line = envelope
        .brief
        .lines()
        .find(|line| line.contains("`src/scoped_dep.rs`"))
        .unwrap_or_else(|| panic!("no bullet for src/scoped_dep.rs: {}", envelope.brief));
    assert!(line.contains("DependencyWidget"), "{line}");
    assert!(line.contains("stage-dependency"), "{line}");
}
