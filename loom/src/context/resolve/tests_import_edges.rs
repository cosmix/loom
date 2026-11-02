//! Tests for resolving import edges onto the files their module paths name.

use super::fixtures::*;
use super::*;
use crate::context::source_graph::{IMPORT_CONFIDENCE, UNRESOLVED_TARGET};

#[test]
fn an_import_resolves_only_when_exactly_one_file_matches() {
    let importing = |symbol: &str| seeking("src/app.ts", SourceEdgeKind::Imports, symbol);

    let mut unique = graph_from(vec![
        ("src/app.ts", &[], importing("./language")),
        ("src/language.ts", &[], vec![]),
    ]);
    let stats = resolve_graph(&mut unique);
    let edge = &unique.files["src/app.ts"].edges[0];
    assert_eq!(edge.to, "src/language.ts");
    assert_eq!(edge.confidence, IMPORT_CONFIDENCE);
    assert_eq!(edge.provenance, EdgeProvenance::Import);
    assert_eq!(stats.retargeted, 1);

    let mut ambiguous = graph_from(vec![
        ("src/app.ts", &[], importing("./language")),
        ("src/a/language.ts", &[], vec![]),
        ("src/b/language.ts", &[], vec![]),
    ]);
    let stats = resolve_graph(&mut ambiguous);
    assert_eq!(ambiguous.files["src/app.ts"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(
        stats.ambiguous, 0,
        "`./language` names src/language.* only; files in other directories are not candidates"
    );
}

#[test]
fn a_crate_rooted_import_drops_the_segment_no_file_starts_with() {
    let mut graph = graph_from(vec![
        ("loom/src/lib.rs", &[], vec![]),
        (
            "loom/src/app.rs",
            &[],
            seeking(
                "loom/src/app.rs",
                SourceEdgeKind::Imports,
                "crate::context::resolve",
            ),
        ),
        ("loom/src/context/resolve.rs", &[], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["loom/src/app.rs"].edges[0].to,
        "loom/src/context/resolve.rs"
    );
    assert_eq!(stats.retargeted, 1);
}

#[test]
fn an_item_path_import_resolves_to_the_file_holding_the_item() {
    let mut graph = graph_from(vec![
        ("src/lib.rs", &[], vec![]),
        (
            "src/app.rs",
            &[],
            seeking("src/app.rs", SourceEdgeKind::Imports, "crate::a::b::Item"),
        ),
        ("src/a/b.rs", &[], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["src/app.rs"].edges[0].to, "src/a/b.rs",
        "the tail of a use path names an item, not a file"
    );
    assert_eq!(
        graph.files["src/app.rs"].edges[0].confidence,
        IMPORT_CONFIDENCE
    );
    assert_eq!(stats.retargeted, 1);
}

#[test]
fn a_crate_import_without_root_evidence_stays_unresolved() {
    let mut graph = graph_from(vec![
        (
            "src/app.rs",
            &[],
            seeking("src/app.rs", SourceEdgeKind::Imports, "crate::a::b::Item"),
        ),
        ("src/a/b.rs", &[], vec![]),
        ("vendor/a/b.rs", &[], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(
        stats,
        expected_stats(0, 0, 1),
        "a missing crate root cannot select a suffix match"
    );
}

#[test]
fn an_import_naming_nothing_in_the_graph_stays_unresolved() {
    let mut graph = graph_from(vec![
        (
            "src/app.rs",
            &[],
            seeking(
                "src/app.rs",
                SourceEdgeKind::Imports,
                "external::totally::unknown::Thing",
            ),
        ),
        ("src/a/b.rs", &[], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(
        (stats.retargeted, stats.ambiguous, stats.unresolved),
        (0, 0, 1)
    );
}
