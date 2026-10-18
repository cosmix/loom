//! Which test nodes a stage's change reaches through the worktree graph.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::context::graph_store::ResolvedGraph;
use crate::context::resolve::{impact_with, ImpactOptions};
use crate::context::source_graph::{file_node_id, SourceNode, UNIQUE_NAME_CONFIDENCE};
use crate::testrun::languages;
use crate::verify::goal_backward::reachable::REACHABLE_KINDS;

/// Test nodes keyed by id.
pub(super) type TestNodes<'g> = BTreeMap<&'g str, &'g SourceNode>;

/// The test nodes a stage's change reaches, split by the evidence of the path.
pub(super) struct Reached<'g> {
    /// Reached through bound edges only, or changed themselves: these run.
    pub(super) selected: TestNodes<'g>,
    /// Test files with a node reached only through a step into an ambiguous
    /// call's candidate set: noted, never run.
    pub(super) namesakes: BTreeSet<String>,
}

/// Every node in a test file that reaches a node of a changed file, or is
/// one, contract files excluded. A test selects only when every step of its
/// path is a bound edge; one reached only by guessing which namesake an
/// ambiguous call means is a namesake note.
pub(super) fn reached_test_nodes<'g>(
    graph: &'g ResolvedGraph,
    changed: &[PathBuf],
    contracts: &BTreeSet<String>,
) -> Reached<'g> {
    let walk = |options: ImpactOptions| {
        let options = ImpactOptions {
            max_depth: usize::MAX,
            kinds: REACHABLE_KINDS.to_vec(),
            ..options
        };
        walk_tests(graph, changed, contracts, &options)
    };
    // A candidate step trusts AMBIGUOUS_CANDIDATE_CONFIDENCE (0.2) and an
    // unbound syntax edge has no target to walk, so the weakest step left is
    // a unique-name bind. A test failing in a package the stage never touched,
    // reached through `validate` matching some other `validate`, would block
    // the stage on a guess; the full suite in integration-verify still runs it.
    let selected = walk(ImpactOptions {
        min_confidence: UNIQUE_NAME_CONFIDENCE,
        follow_candidates: false,
        ..ImpactOptions::default()
    });
    let namesakes = walk(ImpactOptions::default())
        .into_iter()
        .filter(|(id, _)| !selected.contains_key(id))
        .map(|(_, node)| file_node_id(&node.path))
        .collect();
    Reached {
        selected,
        namesakes,
    }
}

/// The test nodes of a changed file, and those reaching a node of a changed
/// file through `options`' edges.
fn walk_tests<'g>(
    graph: &'g ResolvedGraph,
    changed: &[PathBuf],
    contracts: &BTreeSet<String>,
    options: &ImpactOptions,
) -> TestNodes<'g> {
    let is_test = |file: &str| languages::for_path(file).is_some() && !contracts.contains(file);
    // A node an earlier walk reached is not walked again: with no depth limit
    // and a per-step filter, everything reaching it was reached by that walk too.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut reached = TestNodes::new();
    for file in changed.iter().map(|path| file_node_id(path)) {
        let Some(entry) = graph.files.get(&file) else {
            continue;
        };
        for node in &entry.nodes {
            if is_test(&file) {
                reached.insert(node.id.as_str(), node);
            }
            if !seen.insert(node.id.clone()) {
                continue;
            }
            for hit in impact_with(graph, &node.id, options).hits {
                let path = file_node_id(&hit.path);
                if seen.insert(hit.id.clone()) && is_test(&path) {
                    let found = node_in(graph, &path, &hit.id);
                    reached.extend(found.map(|found| (found.id.as_str(), found)));
                }
            }
        }
    }
    reached
}

/// Node `id` of `file`.
fn node_in<'g>(graph: &'g ResolvedGraph, file: &str, id: &str) -> Option<&'g SourceNode> {
    graph
        .files
        .get(file)?
        .nodes
        .iter()
        .find(|node| node.id == id)
}

#[cfg(test)]
#[path = "reach_tests.rs"]
mod tests;
