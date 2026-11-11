//! Which test nodes a change reaches, over a hand-built graph.

use super::*;

/// A common name is no evidence: a test reaching the change only through an
/// ambiguous call's candidate set is noted, while bound paths, including the
/// weakest bind (unique-name), still select.
#[test]
fn a_namesake_reached_only_through_a_candidate_is_noted_not_selected() {
    use crate::context::resolve::fixtures::{edge_at, func_id, graph_from, unresolved_edge};
    use crate::context::source_graph::{EdgeProvenance, SourceEdgeKind, IMPORT_CONFIDENCE};

    let validate = func_id("src/lib.rs", "validate");
    let vendored = func_id("deps/kit/src/kit.rs", "validate");
    let call = |from: &str, provenance, confidence| {
        vec![edge_at(
            from,
            &validate,
            SourceEdgeKind::Calls,
            provenance,
            confidence,
        )]
    };
    let bound = func_id("tests/bound_test.rs", "checks");
    let unique = func_id("tests/unique_test.rs", "checks");
    let namesake = func_id("deps/kit/tests/kit_test.rs", "checks");
    let ambiguous = unresolved_edge(namesake.as_str(), SourceEdgeKind::Calls, "validate")
        .with_candidates(vec![vendored, validate.clone()]);
    let graph = graph_from(vec![
        ("src/lib.rs", &["validate"], vec![]),
        ("deps/kit/src/kit.rs", &["validate"], vec![]),
        (
            "tests/bound_test.rs",
            &["checks"],
            call(&bound, EdgeProvenance::Import, IMPORT_CONFIDENCE),
        ),
        (
            "tests/unique_test.rs",
            &["checks"],
            call(&unique, EdgeProvenance::UniqueName, UNIQUE_NAME_CONFIDENCE),
        ),
        ("deps/kit/tests/kit_test.rs", &["checks"], vec![ambiguous]),
    ]);

    let reached = reached_test_nodes(&graph, &[PathBuf::from("src/lib.rs")], &BTreeSet::new());

    let selected: Vec<&str> = reached.selected.keys().copied().collect();
    assert_eq!(selected, vec![bound.as_str(), unique.as_str()]);
    assert_eq!(
        reached.namesakes,
        BTreeSet::from(["deps/kit/tests/kit_test.rs".to_string()])
    );
}

/// The graph index dominates selection on a large graph, so each of the two
/// walks (selected, namesakes) builds it once, however many nodes changed.
#[test]
fn selection_builds_one_graph_index_per_walk_not_per_changed_node() {
    use crate::context::resolve::fixtures::{edge_at, func_id, graph_from};
    use crate::context::source_graph::{EdgeProvenance, SourceEdgeKind, IMPORT_CONFIDENCE};

    let call = |from: &str, to: &str| {
        vec![edge_at(
            from,
            to,
            SourceEdgeKind::Calls,
            EdgeProvenance::Import,
            IMPORT_CONFIDENCE,
        )]
    };
    let alpha = func_id("src/alpha.rs", "alpha");
    let beta = func_id("src/beta.rs", "beta");
    let checks = func_id("tests/flow_test.rs", "checks");
    let graph = graph_from(vec![
        ("src/alpha.rs", &["alpha", "helper", "other"], vec![]),
        ("src/beta.rs", &["beta", "spare"], call(&beta, &alpha)),
        ("tests/flow_test.rs", &["checks"], call(&checks, &beta)),
    ]);
    let changed = [PathBuf::from("src/alpha.rs"), PathBuf::from("src/beta.rs")];
    let before = ImpactOptions::index_builds();

    let reached = reached_test_nodes(&graph, &changed, &BTreeSet::new());

    assert_eq!(ImpactOptions::index_builds() - before, 2);
    let selected: Vec<&str> = reached.selected.keys().copied().collect();
    assert_eq!(selected, vec![checks.as_str()]);
    assert!(reached.namesakes.is_empty());
}
