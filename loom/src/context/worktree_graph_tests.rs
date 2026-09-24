//! [`super::build_for_worktree`] against a real git repository with a base
//! layer published through the public refresh and store API.

use super::*;
use crate::context::refresh::{reconcile_source_graph, SourceGraphScope};
use crate::context::source_graph::{SourceEdgeKind, SourceNode, SourceNodeKind};
use crate::context::store::ContextStore;
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

    let revision = git_ok(root, &["rev-parse", "HEAD"]);
    let store = ContextStore::with_root(root.join(CACHE_RELATIVE_DIR));
    let graph_store = GraphStore::new(store.root(), &root.join(".loom/work"));
    let scope = SourceGraphScope::Base {
        revision: revision.clone(),
    };
    reconcile_source_graph(&store, &graph_store, root, scope).unwrap();
    assert!(graph_store.base_path(&revision).is_file());
    temp
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
