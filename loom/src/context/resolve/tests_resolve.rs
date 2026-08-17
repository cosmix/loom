//! Tests for cross-file symbol and import resolution.

use super::fixtures::*;
use super::*;
use crate::context::source_graph::UNRESOLVED_TARGET;

/// An unresolved edge of `kind` leaving `from`, naming `symbol`.
fn seeking(from: &str, kind: SourceEdgeKind, symbol: &str) -> Vec<SourceEdge> {
    vec![SourceEdge::unresolved(from, kind, symbol)]
}

#[test]
fn a_unique_cross_file_match_retargets_and_raises_confidence() {
    let caller = func_id("src/caller.rs", "invoke");
    let mut graph = graph_from(vec![
        (
            "src/caller.rs",
            &["invoke"],
            seeking(&caller, SourceEdgeKind::Calls, "target"),
        ),
        ("src/defs.rs", &["target"], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    let edge = &graph.files["src/caller.rs"].edges[0];
    assert_eq!(edge.to, func_id("src/defs.rs", "target"));
    assert_eq!(edge.confidence, UNIQUE_MATCH_CONFIDENCE);
    assert_eq!(
        edge.provenance,
        EdgeProvenance::Inferred,
        "a name match must never present itself as a parse"
    );
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 1,
            ambiguous: 0,
            unresolved: 0
        }
    );
}

#[test]
fn two_definitions_of_one_name_stay_unresolved() {
    let caller = func_id("src/caller.rs", "invoke");
    let mut graph = graph_from(vec![
        (
            "src/caller.rs",
            &["invoke"],
            seeking(&caller, SourceEdgeKind::Calls, "target"),
        ),
        ("src/one.rs", &["target"], vec![]),
        ("src/two.rs", &["target"], vec![]),
    ]);
    let before = graph.files["src/caller.rs"].edges[0].clone();

    let stats = resolve_graph(&mut graph);

    let edge = &graph.files["src/caller.rs"].edges[0];
    assert_eq!(edge.to, UNRESOLVED_TARGET);
    assert_eq!(edge.confidence, before.confidence, "ambiguity buys nothing");
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 0,
            ambiguous: 1,
            unresolved: 1
        }
    );
}

#[test]
fn a_file_stem_colliding_with_a_symbol_is_ambiguity() {
    let mut graph = graph_from(vec![
        (
            "src/app.rs",
            &[],
            seeking("src/app.rs", SourceEdgeKind::References, "language"),
        ),
        ("src/language.rs", &[], vec![]),
        ("src/other.rs", &["language"], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(stats.ambiguous, 1);
}

#[test]
fn a_parser_edge_is_untouched_even_when_a_unique_match_exists() {
    let parser_edge = edge_at(
        func_id("src/caller.rs", "invoke").as_str(),
        UNRESOLVED_TARGET,
        SourceEdgeKind::Calls,
        EdgeProvenance::Parser,
        1.0,
    );
    let mut graph = graph_from(vec![
        ("src/caller.rs", &["invoke"], vec![parser_edge.clone()]),
        ("src/defs.rs", &["target"], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files["src/caller.rs"].edges[0], parser_edge);
    assert_eq!(stats, ResolutionStats::default());
}

#[test]
fn a_containment_edge_is_counted_but_never_guessed_at() {
    let mut graph = graph_from(vec![
        (
            "src/app.rs",
            &[],
            seeking("src/app.rs", SourceEdgeKind::Contains, "orphan"),
        ),
        ("src/defs.rs", &["orphan"], vec![]),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(graph.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 0,
            ambiguous: 0,
            unresolved: 1
        }
    );
}

#[test]
fn an_impl_block_is_not_a_rival_definition_of_its_type() {
    let mut graph = graph_of(vec![
        (
            "src/app.rs",
            source_file(
                "src/app.rs",
                &[],
                seeking("src/app.rs", SourceEdgeKind::References, "Widget"),
            ),
        ),
        (
            "src/defs.rs",
            mixed_file(
                "src/defs.rs",
                &[
                    (SourceNodeKind::Type, "Widget"),
                    (SourceNodeKind::Implementation, "Widget"),
                ],
                vec![],
            ),
        ),
        (
            "src/more.rs",
            mixed_file(
                "src/more.rs",
                &[(SourceNodeKind::Implementation, "Widget")],
                vec![],
            ),
        ),
    ]);

    let stats = resolve_graph(&mut graph);

    assert_eq!(
        graph.files["src/app.rs"].edges[0].to,
        scoped_id("src/defs.rs", SourceNodeKind::Type, "Widget"),
        "an impl block attaches to the type; it does not compete with it"
    );
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 1,
            ambiguous: 0,
            unresolved: 0
        }
    );
}

#[test]
fn a_name_carried_only_by_impl_blocks_resolves_to_nothing() {
    let impls =
        |path: &str| mixed_file(path, &[(SourceNodeKind::Implementation, "Orphan")], vec![]);
    let referrer = || {
        source_file(
            "src/app.rs",
            &[],
            seeking("src/app.rs", SourceEdgeKind::References, "Orphan"),
        )
    };

    let mut contested = graph_of(vec![
        ("src/app.rs", referrer()),
        ("src/one.rs", impls("src/one.rs")),
        ("src/two.rs", impls("src/two.rs")),
    ]);
    let stats = resolve_graph(&mut contested);
    assert_eq!(contested.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 0,
            ambiguous: 1,
            unresolved: 1
        },
        "a name two nodes fought over still reports as contested"
    );

    let mut lone = graph_of(vec![
        ("src/app.rs", referrer()),
        ("src/one.rs", impls("src/one.rs")),
    ]);
    let stats = resolve_graph(&mut lone);
    assert_eq!(lone.files["src/app.rs"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(
        stats,
        ResolutionStats {
            retargeted: 0,
            ambiguous: 0,
            unresolved: 1
        },
        "one impl block is not a contest, just nothing to resolve to"
    );
}

#[test]
fn resolution_is_deterministic_and_idempotent() {
    let caller = func_id("src/caller.rs", "invoke");
    let build = || {
        graph_from(vec![
            (
                "src/caller.rs",
                &["invoke"],
                vec![
                    SourceEdge::unresolved(caller.as_str(), SourceEdgeKind::Calls, "target"),
                    SourceEdge::unresolved(caller.as_str(), SourceEdgeKind::Calls, "nowhere"),
                ],
            ),
            ("src/defs.rs", &["target"], vec![]),
        ])
    };

    let (mut first, mut second) = (build(), build());
    let stats = resolve_graph(&mut first);
    assert_eq!(stats, resolve_graph(&mut second));
    assert_eq!(first, second);

    let again = resolve_graph(&mut first);
    assert_eq!(
        again,
        ResolutionStats {
            retargeted: 0,
            ambiguous: 0,
            unresolved: 1
        },
        "an already-resolved edge is not re-resolved, and the residue is stable"
    );
}

#[test]
fn the_symbol_index_reports_files_under_both_name_and_stem() {
    let graph = graph_from(vec![("src/language.rs", &["detect"], vec![])]);
    let index = SymbolIndex::build(&graph);

    assert_eq!(index.lookup("language.rs"), ["src/language.rs".to_string()]);
    assert_eq!(index.lookup("language"), ["src/language.rs".to_string()]);
    assert_eq!(
        index.lookup("detect"),
        [func_id("src/language.rs", "detect")]
    );
    assert!(index.lookup("absent").is_empty());
}

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
    assert_eq!(edge.confidence, UNIQUE_MATCH_CONFIDENCE);
    assert_eq!(stats.retargeted, 1);

    let mut ambiguous = graph_from(vec![
        ("src/app.ts", &[], importing("./language")),
        ("src/a/language.ts", &[], vec![]),
        ("src/b/language.ts", &[], vec![]),
    ]);
    let stats = resolve_graph(&mut ambiguous);
    assert_eq!(ambiguous.files["src/app.ts"].edges[0].to, UNRESOLVED_TARGET);
    assert_eq!(stats.ambiguous, 1);
}

#[test]
fn a_crate_rooted_import_drops_the_segment_no_file_starts_with() {
    let mut graph = graph_from(vec![
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
        UNIQUE_MATCH_CONFIDENCE
    );
    assert_eq!(stats.retargeted, 1);
}

#[test]
fn a_truncated_import_path_matching_two_files_stays_unresolved() {
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
        ResolutionStats {
            retargeted: 0,
            ambiguous: 1,
            unresolved: 1
        },
        "shortening the path may only widen the candidate set, never decide"
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
