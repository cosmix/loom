//! Tests for `reachable` checks over a worktree graph built in memory from a
//! small Rust crate, so no test depends on git or a published cache layer.

use std::path::{Path, PathBuf};

use super::verify_reachable;
use crate::context::extract::{extract_file, registry};
use crate::context::graph_store::{FileEntry, ResolvedGraph};
use crate::context::resolve::resolve_graph;
use crate::context::source_graph::body_hash;
use crate::context::worktree_graph::WorktreeGraph;
use crate::plan::schema::ReachableCheck;
use crate::verify::goal_backward::{GapType, VerificationGap};

/// `main -> run` is a same-file parser edge; `run -> helper` crosses files and
/// is resolved by name. `orphan` sits in the file named `main` but nothing
/// calls it.
const MAIN_RS: &str = "mod helpers;

use helpers::helper;

fn main() {
    run();
}

fn run() {
    helper();
}

fn orphan() {}
";

const HELPERS_RS: &str = "pub fn helper() {}\n";

const DEPLOY_SH: &str = "#!/bin/sh\ncargo build --release\n";

fn crate_graph(degraded: Option<&str>) -> WorktreeGraph {
    let files = [
        ("src/main.rs", MAIN_RS),
        ("src/helpers.rs", HELPERS_RS),
        ("scripts/deploy.sh", DEPLOY_SH),
    ];
    let extractors = registry();
    let mut graph = ResolvedGraph::default();
    for (path, source) in files {
        let extraction = extract_file(&extractors, Path::new(path), source.as_bytes());
        let entry = FileEntry {
            content_hash: body_hash(source.as_bytes()),
            nodes: extraction.nodes,
            edges: extraction.edges,
            coverage: extraction.coverage,
        };
        graph.files.insert(path.to_string(), entry);
    }
    resolve_graph(&mut graph);
    WorktreeGraph {
        graph,
        degraded: degraded.map(str::to_string),
        changed: files.iter().map(|&(path, _)| PathBuf::from(path)).collect(),
    }
}

fn check(symbol: &str, from: &str, min_confidence: Option<f32>) -> ReachableCheck {
    ReachableCheck {
        symbol: symbol.to_string(),
        from: from.to_string(),
        min_confidence,
        description: format!("{symbol} is wired into {from}"),
    }
}

fn single_gap(gaps: Vec<VerificationGap>) -> VerificationGap {
    assert_eq!(gaps.len(), 1, "expected exactly one gap, got {gaps:?}");
    gaps.into_iter().next().expect("one gap")
}

#[test]
fn reachable_through_call_chain_passes() {
    let gaps = verify_reachable(&[check("helper", "main", None)], &crate_graph(None));
    assert!(gaps.is_empty(), "unexpected gaps: {gaps:?}");
}

#[test]
fn unreachable_symbol_is_a_gap() {
    let degraded = "no published base layer; extracted every tracked source file";
    let gap = single_gap(verify_reachable(
        &[check("orphan", "main", None)],
        &crate_graph(Some(degraded)),
    ));
    assert!(matches!(gap.gap_type, GapType::Unreachable));
    assert!(
        gap.description
            .contains("orphan is not reachable from main"),
        "{}",
        gap.description
    );
    assert!(gap.description.contains(degraded), "{}", gap.description);
}

#[test]
fn missing_node_names_the_symbol() {
    let gap = single_gap(verify_reachable(
        &[check("helper", "entrypoint", None)],
        &crate_graph(None),
    ));
    assert!(matches!(gap.gap_type, GapType::Unreachable));
    assert!(
        gap.description.starts_with("symbol not found: entrypoint"),
        "{}",
        gap.description
    );
}

#[test]
fn path_below_min_confidence_is_a_gap() {
    // `run -> helper` is resolved by name, so it stays below 0.95.
    let gap = single_gap(verify_reachable(
        &[check("helper", "main", Some(0.95))],
        &crate_graph(None),
    ));
    assert!(gap
        .description
        .contains("helper is not reachable from main"));
}

#[test]
fn symbol_without_extractor_is_skipped() {
    let gaps = verify_reachable(&[check("deploy", "main", None)], &crate_graph(None));
    assert!(gaps.is_empty(), "unexpected gaps: {gaps:?}");
}
