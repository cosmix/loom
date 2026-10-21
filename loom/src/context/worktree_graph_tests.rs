//! [`super::build_for_worktree`] against a real git repository with a base
//! layer published through the public refresh and store API.

use super::*;
use crate::context::refresh::{reconcile_source_graph, SourceGraphScope};
use crate::context::source_graph::{
    SourceEdgeKind, SourceNode, SourceNodeKind, MAX_EXTRACTED_FILE_BYTES,
};
use crate::context::store::ContextStore;
use crate::git::runner::run_git_checked;
use crate::verify::contracts::test_support::{contract_worktree, plant_foreign_git_dir};
use std::fs;
use tempfile::TempDir;

/// Run one git setup command with ambient global/system config neutralized
/// and assert it succeeded; returns trimmed stdout.
fn git_ok(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository with `base.rs` committed and a base layer published for
/// `HEAD` into its own `.loom/cache`.
fn repo_with_published_base() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git_ok(root, &["init", "-b", "main"]);
    git_ok(root, &["config", "user.email", "t@t.com"]);
    git_ok(root, &["config", "user.name", "t"]);
    fs::write(root.join("base.rs"), "pub fn helper() -> u32 {\n    7\n}\n").unwrap();
    git_ok(root, &["add", "base.rs"]);
    git_ok(root, &["commit", "-m", "seed"]);
    publish_head_base(root);
    temp
}

/// Publish a base layer for `HEAD` into `root`'s own `.loom/cache`.
fn publish_head_base(root: &Path) {
    let revision = git_ok(root, &["rev-parse", "HEAD"]);
    let store = ContextStore::with_root(root.join(CACHE_RELATIVE_DIR));
    let graph_store = GraphStore::new(store.root(), &root.join(".loom/work"));
    let scope = SourceGraphScope::Base {
        revision: revision.clone(),
    };
    reconcile_source_graph(&store, &graph_store, root, scope).unwrap();
    assert!(graph_store.base_path(&revision).is_file());
}

/// Every file under `dir` with its bytes, keyed by path.
fn tree_bytes(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(tree_bytes(&path));
        } else {
            let bytes = fs::read(&path).unwrap();
            found.insert(path, bytes);
        }
    }
    found
}

fn function_named<'a>(graph: &'a ResolvedGraph, file: &str, name: &str) -> &'a SourceNode {
    graph
        .nodes()
        .find(|node| {
            node.kind == SourceNodeKind::Function
                && node.path == Path::new(file)
                && node.scope.last().map(String::as_str) == Some(name)
        })
        .unwrap_or_else(|| panic!("no function {name} in {file}"))
}

#[test]
fn worktree_graph_includes_uncommitted_new_file() {
    let temp = repo_with_published_base();
    let root = temp.path();
    let cache = root.join(".loom");
    let before = tree_bytes(&cache);
    fs::write(
        root.join("consumer.rs"),
        "pub fn consume() -> u32 {\n    helper()\n}\n",
    )
    .unwrap();

    let built = build_for_worktree(root).unwrap();

    assert_eq!(built.degraded, None);
    assert_eq!(built.changed, vec![PathBuf::from("consumer.rs")]);
    assert!(built.graph.files.contains_key("consumer.rs"));
    let consume = function_named(&built.graph, "consumer.rs", "consume");
    let helper = function_named(&built.graph, "base.rs", "helper");
    assert!(
        built
            .graph
            .edges()
            .any(|edge| edge.kind == SourceEdgeKind::Calls
                && edge.from == consume.id
                && edge.to == helper.id),
        "no resolved call edge from {} to {}",
        consume.id,
        helper.id
    );
    assert_eq!(tree_bytes(&cache), before, "the cache was written to");
}

/// Rewrite the published base for `HEAD` in place, the way an older build left it.
fn rewrite_base(root: &Path, edit: impl FnOnce(&mut GraphLayer)) {
    let revision = git_ok(root, &["rev-parse", "HEAD"]);
    let store = ContextStore::with_root(root.join(CACHE_RELATIVE_DIR));
    let graph_store = GraphStore::new(store.root(), &root.join(".loom/work"));
    let mut layer = graph_store.load_base(&revision).unwrap().unwrap();
    edit(&mut layer);
    fs::write(
        graph_store.base_path(&revision),
        serde_json::to_vec(&layer).unwrap(),
    )
    .unwrap();
}

#[test]
fn stale_parser_base_entry_is_reextracted() {
    let temp = repo_with_published_base();
    let root = temp.path();
    rewrite_base(root, |layer| {
        for node in &mut layer.files.get_mut("base.rs").unwrap().nodes {
            node.parser_version = "retired-extractor@0".to_string();
        }
    });

    let built = build_for_worktree(root).unwrap();

    assert_eq!(built.degraded, None);
    assert!(built.changed.is_empty(), "the tree is clean");
    let entry = &built.graph.files["base.rs"];
    assert!(
        entry
            .nodes
            .iter()
            .all(|node| node.parser_version != "retired-extractor@0"),
        "the stale base entry was served"
    );
    assert!(parser_version_matches(
        entry,
        &extract::registry(),
        Path::new("base.rs")
    ));
    assert!(built.graph.overlaid.contains("base.rs"));
}

#[test]
fn stale_schema_base_degrades_to_a_scratch_extraction() {
    let temp = repo_with_published_base();
    let root = temp.path();
    rewrite_base(root, |layer| layer.schema_version = 0);

    let built = build_for_worktree(root).unwrap();

    assert!(built.degraded.is_some(), "a stale-schema base was trusted");
    assert!(built.graph.files.contains_key("base.rs"));
}

#[test]
fn oversized_base_entry_is_kept_not_reextracted() {
    let temp = repo_with_published_base();
    let root = temp.path();
    let oversized = "\n".repeat(MAX_EXTRACTED_FILE_BYTES + 1);
    fs::write(root.join("big.rs"), oversized).unwrap();
    git_ok(root, &["add", "big.rs"]);
    git_ok(root, &["commit", "-m", "add oversized"]);
    publish_head_base(root);

    let built = build_for_worktree(root).unwrap();

    assert_eq!(built.degraded, None);
    assert!(built.changed.is_empty(), "the tree is clean");
    let coverage = &built.graph.files["big.rs"].coverage;
    assert!(
        matches!(coverage, FileCoverage::Oversized { .. }),
        "the oversized entry was re-extracted: {coverage:?}"
    );
    assert!(!built.graph.overlaid.contains("big.rs"));
}

#[test]
fn a_changed_oversized_file_keeps_its_file_node_as_the_refresh_path_does() {
    let temp = repo_with_published_base();
    let root = temp.path();
    fs::write(
        root.join("big.rs"),
        "\n".repeat(MAX_EXTRACTED_FILE_BYTES + 1),
    )
    .unwrap();

    let built = build_for_worktree(root).unwrap();

    assert_eq!(built.changed, vec![PathBuf::from("big.rs")]);
    let entry = &built.graph.files["big.rs"];
    assert!(
        matches!(entry.coverage, FileCoverage::Oversized { .. }),
        "a changed oversized file became {:?}",
        entry.coverage
    );
    assert!(
        entry
            .nodes
            .iter()
            .any(|node| node.kind == SourceNodeKind::File),
        "the oversized file lost its file node"
    );
    assert!(!entry.content_hash.is_empty());
}

/// Materialize the base view for `HEAD` in `root`'s own cache; returns the
/// view directory.
fn materialize_head_view(root: &Path) -> PathBuf {
    let revision = git_ok(root, &["rev-parse", "HEAD"]);
    let graph_store = read_only_store(root).unwrap();
    graph_store.view(&revision, None).unwrap();
    let view_dir = graph_store.view_dir();
    assert!(view_dir.is_dir(), "no view was persisted");
    view_dir
}

#[test]
fn worktree_graph_relinked_from_the_base_view_equals_a_cold_build() {
    let temp = repo_with_published_base();
    let root = temp.path();
    let view_dir = materialize_head_view(root);
    fs::write(
        root.join("consumer.rs"),
        "pub fn consume() -> u32 {\n    helper()\n}\n",
    )
    .unwrap();
    let cache = root.join(".loom");
    let before = tree_bytes(&cache);

    let relinked = build_for_worktree(root).unwrap();

    assert_eq!(tree_bytes(&cache), before, "the cache was written to");
    fs::remove_dir_all(&view_dir).unwrap();
    let cold = build_for_worktree(root).unwrap();
    assert_eq!(relinked, cold);
    assert!(relinked.graph.files.contains_key("consumer.rs"));
}

#[test]
fn a_base_view_of_stale_parser_entries_is_not_copied_into_the_worktree_graph() {
    let temp = repo_with_published_base();
    let root = temp.path();
    materialize_head_view(root);
    rewrite_base(root, |layer| {
        for node in &mut layer.files.get_mut("base.rs").unwrap().nodes {
            node.parser_version = "retired-extractor@0".to_string();
        }
    });

    let built = build_for_worktree(root).unwrap();

    assert!(
        built.graph.files["base.rs"]
            .nodes
            .iter()
            .all(|node| node.parser_version != "retired-extractor@0"),
        "the stale base entry was served"
    );
}

/// The positive control runs last: git that follows the repointed `.git`
/// file runs the filter and reports the planted directory as the common
/// directory, so the unchanged graph before comes from the pin.
#[test]
fn a_repointed_git_file_neither_runs_its_filter_nor_moves_the_graph() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    publish_head_base(&repo);
    let git_file = worktree.join(".git");
    let registered = fs::read(&git_file).unwrap();
    let marker = plant_foreign_git_dir(&worktree);
    let planted = fs::read(&git_file).unwrap();

    let attacked = build_for_worktree(&worktree).unwrap();

    assert!(!marker.exists(), "the graph ran the stage's clean filter");
    fs::write(&git_file, &registered).unwrap();
    let honest = build_for_worktree(&worktree).unwrap();
    assert_eq!(attacked, honest);
    assert_eq!(honest.degraded, None, "the project's base layer was missed");
    assert!(honest.changed.contains(&PathBuf::from("README.md")));
    assert!(honest
        .changed
        .contains(&PathBuf::from("tests/x_contract.rs")));

    fs::write(&git_file, &planted).unwrap();
    let common = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    let followed = run_git_checked(&common, &worktree).unwrap();
    assert_ne!(
        Path::new(&followed),
        repo.join(".git").canonicalize().unwrap()
    );
    run_git_checked(&["status", "--porcelain"], &worktree).unwrap();
    assert!(marker.exists(), "the fixture's filter never runs");
}

#[test]
fn a_scratch_extraction_lists_only_the_working_directory_literally() {
    let tmp = TempDir::new().unwrap();
    let worktree = contract_worktree(&tmp.path().join("repo"), "s1");
    for file in ["pkg[1]/a.rs", "pkg1/b.rs", "other/c.rs"] {
        fs::create_dir_all(worktree.join(file).parent().unwrap()).unwrap();
        fs::write(worktree.join(file), "pub fn f() {}\n").unwrap();
    }

    let nested = build_for_worktree(&worktree.join("pkg[1]")).unwrap();
    let whole = build_for_worktree(&worktree).unwrap();

    assert!(nested.degraded.is_some(), "no base layer is published");
    let nested_files: Vec<&str> = nested.graph.files.keys().map(String::as_str).collect();
    assert_eq!(nested_files, ["pkg[1]/a.rs"]);
    assert!(whole.graph.files.contains_key("other/c.rs"));
    assert!(whole.graph.files.contains_key("pkg1/b.rs"));
}
