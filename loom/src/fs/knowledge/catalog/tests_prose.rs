//! Tests for prose indexing (`catalog::prose`).
//!
//! Fixture shape every test in this file shares, because
//! `prose::project_root_of` requires exactly this layout (see its doc
//! comment): a knowledge tree at `doc/loom/knowledge`, curated content
//! there, and prose elsewhere under `doc/`.
//!
//! ```text
//! <temp>/doc/loom/knowledge/architecture.md   curated
//! <temp>/doc/design.md                        prose, indexed
//! <temp>/doc/plans/PLAN-live.md                prose, indexed
//! <temp>/doc/plans/DONE-PLAN-old.md           NOT indexed
//! ```

use crate::context::config::RetrievalConfig;
use crate::context::rank::{rank_channel, RankQuery};
use crate::context::retrieve::reject_unknown_require_ids;
use crate::context::schema::Channel;
use crate::context::source_graph::MAX_EXTRACTED_FILE_BYTES;
use crate::fs::knowledge::catalog;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Write `contents` to `root/relative`, creating parent directories as
/// needed. Mirrors the helper at `loom/src/context/tests/ingest.rs:7`.
fn write_file(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn prose_files_are_indexed_with_a_prose_prefixed_id() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nCurated architecture body.\n",
    );
    write_file(
        root,
        "doc/design.md",
        "## Design\n\nThe cadastre orthophoto tiles project is described here.\n",
    );
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();

    let prose_chunk = catalog
        .chunks
        .iter()
        .find(|chunk| chunk.id.starts_with("prose:doc/design.md#"))
        .expect("expected a prose chunk for doc/design.md");
    assert!(prose_chunk.body.contains("cadastre orthophoto"));
}

#[test]
fn the_cadastre_probe_returns_the_prose_chunk() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nBody about the orchestrator loop.\n",
    );
    write_file(
        root,
        "doc/design.md",
        "## Design\n\nThe cadastre orthophoto imagery tiles are reviewed in this section.\n",
    );
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();
    let ranking = rank_channel(
        &RankQuery {
            text: "cadastre orthophoto imagery".into(),
            ..Default::default()
        },
        &catalog.chunks,
        Channel::Knowledge,
        &RetrievalConfig::default(),
    );

    let ids: Vec<&str> = ranking
        .candidates
        .iter()
        .map(|candidate| candidate.id.as_str())
        .collect();
    assert!(
        ids.iter().any(|id| id.starts_with("prose:doc/design.md#")),
        "expected the prose chunk among ranked candidates, got: {ids:?}"
    );
}

#[test]
fn a_done_plan_is_not_indexed() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nBody.\n",
    );
    write_file(
        root,
        "doc/plans/DONE-PLAN-old.md",
        "## Old Plan\n\nCompleted plan body.\n",
    );
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();

    assert!(!catalog
        .chunks
        .iter()
        .any(|chunk| chunk.id.contains("DONE-PLAN-old")));
}

#[test]
fn a_live_plan_is_indexed() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nBody.\n",
    );
    write_file(
        root,
        "doc/plans/PLAN-live.md",
        "## Live Plan\n\nIn-progress plan body.\n",
    );
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();

    assert!(catalog
        .chunks
        .iter()
        .any(|chunk| chunk.id.starts_with("prose:doc/plans/PLAN-live.md#")));
}

#[test]
fn the_curated_tree_is_not_indexed_twice() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nA unique curated body that must not be duplicated.\n",
    );
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();

    assert!(!catalog
        .chunks
        .iter()
        .any(|chunk| chunk.id.starts_with("prose:doc/loom/knowledge/")));

    let mut bodies: Vec<&str> = catalog
        .chunks
        .iter()
        .map(|chunk| chunk.body.as_str())
        .collect();
    let before = bodies.len();
    bodies.sort_unstable();
    bodies.dedup();
    assert_eq!(
        bodies.len(),
        before,
        "a curated body was indexed a second time as prose"
    );
}

#[test]
fn an_oversized_prose_file_is_skipped() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nBody.\n",
    );
    let oversized = vec![b'#'; MAX_EXTRACTED_FILE_BYTES + 1];
    let huge_path = root.join("doc/huge.md");
    fs::create_dir_all(huge_path.parent().unwrap()).unwrap();
    fs::write(&huge_path, &oversized).unwrap();
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();

    assert!(!catalog
        .chunks
        .iter()
        .any(|chunk| chunk.id.starts_with("prose:doc/huge.md#")));
}

#[test]
fn empty_prose_roots_disable_indexing() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(root, ".loom/config.toml", "[retrieval]\nprose_roots = []\n");
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nBody.\n",
    );
    write_file(root, "doc/design.md", "## Design\n\nBody.\n");
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();

    assert!(catalog
        .chunks
        .iter()
        .all(|chunk| !chunk.id.starts_with("prose:")));
    assert_eq!(catalog.chunks.len(), 1);
}

#[test]
fn an_absent_prose_root_is_silent() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        ".loom/config.toml",
        "[retrieval]\nprose_roots = [\"not-here\"]\n",
    );
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nBody.\n",
    );
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();

    assert_eq!(catalog.chunks.len(), 1);
    assert!(catalog.issues.is_empty());
}

#[test]
fn a_prose_chunk_id_is_accepted_by_require_id() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write_file(
        root,
        "doc/loom/knowledge/architecture.md",
        "## Curated\n\nBody.\n",
    );
    write_file(
        root,
        "doc/design.md",
        "## Design\n\nBody about a design decision.\n",
    );
    let knowledge_root = root.join("doc/loom/knowledge");

    let catalog = catalog::build(&knowledge_root).unwrap();
    let prose_id = catalog
        .chunks
        .iter()
        .find(|chunk| chunk.id.starts_with("prose:doc/design.md#"))
        .expect("expected a prose chunk")
        .id
        .clone();

    let result = reject_unknown_require_ids(&catalog, None, &[prose_id]);

    assert!(result.is_ok());
}
