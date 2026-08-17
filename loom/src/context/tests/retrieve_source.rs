//! End-to-end coverage: a real source-graph overlay on disk, read all the way
//! through `retrieve_for_stage` into a packed `ItemKind::SourceNode` item.
//!
//! Every other source-channel test builds its `ResolvedGraph` by hand and
//! calls `pack`/`rank_source` directly (see `pack_source.rs`,
//! `rank_source.rs`, `rank_source_matching.rs`). None of them exercise
//! `load_resolved_graph`'s own wiring — the overlay address it computes, or
//! `retrieve_for_stage`'s call into it. If `load_resolved_graph` degraded to
//! `None` on every call, or resolved the wrong `(plan, stage)` address, every
//! one of those tests would still pass unchanged. This test writes a real
//! overlay file at the exact address `OverlayScope::Local` resolves to and
//! drives the whole pipeline against it, so it fails on either mistake, and
//! it fails too if `pack` ever stops emitting source items.

use super::retrieve::project_with_knowledge;
use crate::context::graph_store::{FileEntry, GraphLayer, GraphStore};
use crate::context::local_overlay::local_overlay_key;
use crate::context::retrieve::{retrieve_for_stage, StageQuery};
use crate::context::schema::*;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A symbol distinctive enough that it cannot collide with anything in
/// `project_with_knowledge`'s prose, so a hit in the pack can only have come
/// from the source graph this test writes.
const DISTINCTIVE_SYMBOL: &str = "ZorbleFrobnicator";

/// One symbol node with a real, non-default span and signature.
fn distinctive_node() -> SourceNode {
    SourceNode {
        id: "src/zorble.rs#function:ZorbleFrobnicator".to_string(),
        kind: SourceNodeKind::Function,
        path: PathBuf::from("src/zorble.rs"),
        scope: vec![DISTINCTIVE_SYMBOL.to_string()],
        span: Span {
            start_byte: 40,
            end_byte: 96,
            line_start: 12,
            line_end: 14,
        },
        signature: "pub fn zorble_frobnicator() -> Widget".to_string(),
        body_hash: "sha256:zorble".to_string(),
        language: NodeLanguage::Rust,
        parser_version: "test+v1".to_string(),
        coverage: FileCoverage::Full,
    }
}

/// Write `node` as a real overlay layer at the address `OverlayScope::Local`
/// resolves for `root`, using the same API the production writer (`loom
/// map`, `commands/map.rs`) uses to write it.
fn write_local_overlay(root: &Path, node: &SourceNode) {
    let work_dir = WorkDir::new(root).unwrap();
    let project_root = work_dir.project_root().unwrap();
    let store = ContextStore::open(&work_dir).unwrap();
    let graph_store = GraphStore::new(store.root(), work_dir.root());
    let (plan, stage) = local_overlay_key(project_root);

    let mut files = BTreeMap::new();
    files.insert(
        node.path.to_string_lossy().into_owned(),
        FileEntry {
            content_hash: "sha256:file".to_string(),
            nodes: vec![node.clone()],
            edges: Vec::new(),
            coverage: FileCoverage::Full,
        },
    );
    let layer = GraphLayer {
        revision: "test-revision".to_string(),
        built_at: None,
        files,
    };
    graph_store.save_overlay(&plan, &stage, &layer).unwrap();
}

#[test]
fn retrieve_for_stage_packs_a_source_node_from_a_real_overlay_on_disk() {
    let temp = project_with_knowledge();
    let root = temp.path();
    let node = distinctive_node();
    write_local_overlay(root, &node);

    let query = StageQuery::new(root, format!("Where is {DISTINCTIVE_SYMBOL} defined?"));
    let pack = retrieve_for_stage(&query, 500).unwrap();

    let item = pack
        .items
        .iter()
        .find(|item| item.id.as_str() == node.id)
        .unwrap_or_else(|| {
            panic!(
                "expected a source item for {}, got {:?}",
                node.id,
                pack.items
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>()
            )
        });

    assert_eq!(item.kind, ItemKind::SourceNode);
    assert_eq!(item.pointer.path, node.path);
    assert_eq!(item.pointer.line_start, Some(12));
    assert_eq!(item.pointer.line_end, Some(14));
}
