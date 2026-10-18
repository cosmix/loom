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
