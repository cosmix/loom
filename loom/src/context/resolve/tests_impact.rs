//! Tests for the bounded reverse traversal and its minimum-confidence rule.

use super::*;
use crate::context::resolve::fixtures::*;
use crate::context::resolve::UNIQUE_MATCH_CONFIDENCE;
use crate::context::source_graph::UNRESOLVED_TARGET;

/// A fully-parsed call edge, the trustworthy baseline for traversal tests.
fn call(from: &str, to: &str) -> Vec<SourceEdge> {
    vec![edge_at(
        from,
        to,
        SourceEdgeKind::Calls,
        EdgeProvenance::Parser,
        1.0,
    )]
}

fn ids(hits: &[ImpactHit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.id.as_str()).collect()
}

/// `a -> b -> c -> d` as parser call edges, each stored with its caller. Every
/// file holds one function named after it, so `b` lives in `src/b.rs`.
fn chain() -> ResolvedGraph {
    let link = |from: &str, to: &str| {
        call(
            &func_id(&format!("src/{from}.rs"), from),
            &func_id(&format!("src/{to}.rs"), to),
        )
    };
    graph_from(vec![
        ("src/a.rs", &["a"], link("a", "b")),
        ("src/b.rs", &["b"], link("b", "c")),
        ("src/c.rs", &["c"], link("c", "d")),
        ("src/d.rs", &["d"], vec![]),
    ])
}

#[test]
fn impact_stops_at_max_depth() {
    let graph = chain();
    let hits = impact(&graph, &func_id("src/d.rs", "d"), 2);

    assert_eq!(
        ids(&hits),
        vec![
            func_id("src/c.rs", "c").as_str(),
            func_id("src/b.rs", "b").as_str()
        ]
    );
    assert_eq!(hits[0].depth, 1);
    assert_eq!(hits[1].depth, 2);
    assert_eq!(hits[0].kind, SourceNodeKind::Function);
    assert_eq!(hits[0].path, PathBuf::from("src/c.rs"));
}

#[test]
fn impact_of_depth_zero_reaches_nothing() {
    let graph = chain();
    assert!(impact(&graph, &func_id("src/d.rs", "d"), 0).is_empty());
}

/// `w -> x -> y -> z`, with the three hops at descending confidence
/// 0.75, 0.5, 1.0 — deliberately NOT monotonic, so a path-confidence rule that
/// took the last hop or multiplied the chain would give a different answer from
/// the minimum at every node.
fn descending_confidence_chain() -> crate::context::graph_store::ResolvedGraph {
    let hop =
        |from: &str,
         to: &str,
         kind: SourceEdgeKind,
         provenance: EdgeProvenance,
         confidence: f32| { vec![edge_at(from, to, kind, provenance, confidence)] };
    graph_from(vec![
        (
            "src/w.rs",
            &["w"],
            hop(
                "src/w.rs#function:w",
                "src/x.rs#function:x",
                SourceEdgeKind::References,
                EdgeProvenance::Inferred,
                UNIQUE_MATCH_CONFIDENCE,
            ),
        ),
        (
            "src/x.rs",
            &["x"],
            hop(
                "src/x.rs#function:x",
                "src/y.rs#function:y",
                SourceEdgeKind::Imports,
                EdgeProvenance::Inferred,
                0.5,
            ),
        ),
        (
            "src/y.rs",
            &["y"],
            hop(
                "src/y.rs#function:y",
                "src/z.rs#function:z",
                SourceEdgeKind::Calls,
                EdgeProvenance::Parser,
                1.0,
            ),
        ),
        ("src/z.rs", &["z"], vec![]),
    ])
}

#[test]
fn path_confidence_is_the_minimum_not_the_product() {
    let graph = descending_confidence_chain();

    let hits = impact(&graph, "src/z.rs#function:z", 3);

    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].id, "src/y.rs#function:y");
    assert_eq!(hits[0].min_confidence, 1.0);
    assert_eq!(hits[0].weakest_provenance, EdgeProvenance::Parser);

    assert_eq!(hits[1].id, "src/x.rs#function:x");
    assert_eq!(hits[1].min_confidence, 0.5);

    let farthest = &hits[2];
    assert_eq!(farthest.id, "src/w.rs#function:w");
    assert_eq!(
        farthest.min_confidence, 0.5,
        "0.5 is the weakest step; 0.375 would be a product and 0.75 the last hop"
    );
    assert_eq!(farthest.weakest_provenance, EdgeProvenance::Inferred);
    assert_eq!(farthest.weakest_kind, SourceEdgeKind::Imports);
}

#[test]
fn a_cycle_terminates_and_reports_each_node_once() {
    let graph = graph_from(vec![
        ("src/a.rs", &[], call("src/a.rs", "src/b.rs")),
        ("src/b.rs", &[], call("src/b.rs", "src/c.rs")),
        ("src/c.rs", &[], call("src/c.rs", "src/a.rs")),
    ]);

    let hits = impact(&graph, "src/a.rs", 10);

    assert_eq!(
        ids(&hits),
        vec!["src/c.rs", "src/b.rs"],
        "the start node is the subject of the query, never a result"
    );
}

#[test]
fn impact_ignores_unresolved_edges_and_absent_origins() {
    let graph = graph_from(vec![(
        "src/app.rs",
        &[],
        vec![
            SourceEdge::unresolved("src/app.rs", SourceEdgeKind::Calls, "mystery"),
            edge_at(
                "src/deleted.rs",
                "src/app.rs",
                SourceEdgeKind::Calls,
                EdgeProvenance::Parser,
                1.0,
            ),
        ],
    )]);

    assert!(
        impact(&graph, UNRESOLVED_TARGET, 3).is_empty(),
        "the unresolved placeholder must not become a hub joining every guess"
    );
    assert!(
        impact(&graph, "src/app.rs", 3).is_empty(),
        "an edge from a node the graph does not contain reaches nothing"
    );
}
