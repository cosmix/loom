//! Tests for the multi-start reverse traversal and its single graph index.

use super::*;
use crate::context::resolve::fixtures::*;
use crate::context::source_graph::{SourceEdge, LOCAL_NAME_CONFIDENCE};

fn f(name: &str) -> String {
    func_id(&format!("src/{name}.rs"), name)
}

fn link(from: &str, to: &str) -> SourceEdge {
    edge_at(
        &f(from),
        &f(to),
        SourceEdgeKind::Calls,
        EdgeProvenance::LocalName,
        LOCAL_NAME_CONFIDENCE,
    )
}

/// `a -> b -> c -> d`, `e -> d`, `b -> e`, and `g` calling an ambiguous `c`
/// whose candidates are `c` and `x`.
fn web() -> ResolvedGraph {
    let ambiguous =
        unresolved_edge(f("g"), SourceEdgeKind::Calls, "c").with_candidates(vec![f("c"), f("x")]);
    graph_from(vec![
        ("src/a.rs", &["a"], vec![link("a", "b")]),
        ("src/b.rs", &["b"], vec![link("b", "c"), link("b", "e")]),
        ("src/c.rs", &["c"], vec![link("c", "d")]),
        ("src/d.rs", &["d"], vec![]),
        ("src/e.rs", &["e"], vec![link("e", "d")]),
        ("src/g.rs", &["g"], vec![ambiguous]),
        ("src/x.rs", &["x"], vec![]),
    ])
}

fn hit_ids(result: &ImpactResult) -> BTreeSet<String> {
    result.hits.iter().map(|hit| hit.id.clone()).collect()
}

fn unbounded(follow_candidates: bool) -> ImpactOptions {
    ImpactOptions {
        max_depth: usize::MAX,
        follow_candidates,
        ..ImpactOptions::default()
    }
}

#[test]
fn a_multi_start_walk_reaches_the_union_of_single_start_walks_less_the_starts() {
    let graph = web();
    let starts = [f("d"), f("x"), f("c")];
    let start_refs: Vec<&str> = starts.iter().map(String::as_str).collect();
    for options in [unbounded(true), unbounded(false)] {
        let mut union: BTreeSet<String> = start_refs
            .iter()
            .flat_map(|start| hit_ids(&impact_with(&graph, start, &options)))
            .collect();
        for start in &starts {
            union.remove(start);
        }

        let all = options.impact_from(&graph, &start_refs);

        assert_eq!(hit_ids(&all), union, "options: {options:?}");
    }
}

#[test]
fn a_multi_start_hit_takes_the_depth_from_its_nearest_start() {
    let graph = web();

    let all = unbounded(false).impact_from(&graph, &[&f("d"), &f("b")]);

    let depth = |id: &str| {
        all.hits
            .iter()
            .find(|hit| hit.id == id)
            .map(|hit| hit.depth)
    };
    assert_eq!(depth(&f("a")), Some(1));
    assert_eq!(depth(&f("c")), Some(1));
    assert_eq!(depth(&f("e")), Some(1));
    assert_eq!(depth(&f("b")), None, "a start is never a hit");
}

#[test]
fn a_multi_start_walk_builds_the_graph_index_once() {
    let graph = web();
    let before = ImpactOptions::index_builds();

    let all = unbounded(true).impact_from(&graph, &[&f("d"), &f("x"), &f("e"), &f("c")]);

    assert!(!all.hits.is_empty());
    assert_eq!(ImpactOptions::index_builds() - before, 1);
}

#[test]
fn a_multi_start_walk_of_depth_zero_builds_nothing_and_reaches_nothing() {
    let graph = web();
    let before = ImpactOptions::index_builds();
    let options = ImpactOptions {
        max_depth: 0,
        ..ImpactOptions::default()
    };

    let all = options.impact_from(&graph, &[&f("d")]);

    assert!(all.hits.is_empty());
    assert_eq!(ImpactOptions::index_builds(), before);
}
