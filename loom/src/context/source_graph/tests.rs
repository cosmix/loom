use super::*;
use std::path::Path;

#[test]
fn inferred_confidence_is_clamped_to_the_ceiling() {
    let edge = SourceEdge::inferred("a", "b", SourceEdgeKind::Calls, "f", 0.99);
    assert_eq!(edge.confidence, MAX_INFERRED_CONFIDENCE);
    assert_eq!(edge.provenance, EdgeProvenance::Inferred);
}

#[test]
fn a_parser_edge_is_fully_confident_and_resolved() {
    let edge = SourceEdge::parser("a", "b", SourceEdgeKind::Contains, "f");
    assert_eq!(edge.confidence, 1.0);
    assert!(!edge.is_unresolved());
}

#[test]
fn an_unresolved_edge_names_the_symbol_it_could_not_find() {
    let edge = SourceEdge::unresolved("a", SourceEdgeKind::Calls, "dynamic_target");
    assert!(edge.is_unresolved());
    assert_eq!(edge.symbol, "dynamic_target");
    assert!(edge.confidence <= MAX_INFERRED_CONFIDENCE);
}

#[test]
fn ids_are_forward_slashed_and_scope_joined() {
    assert_eq!(
        file_node_id(Path::new("src/context/mod.rs")),
        "src/context/mod.rs"
    );
    assert_eq!(
        node_id(
            Path::new("src/lib.rs"),
            SourceNodeKind::Function,
            &["Outer".to_string(), "inner".to_string()]
        ),
        "src/lib.rs#function:Outer::inner"
    );
}

#[test]
fn resolution_raises_a_unique_match_above_the_extraction_ceiling() {
    let mut edge = SourceEdge::unresolved("a", SourceEdgeKind::Calls, "helper");
    assert!(edge.resolve_to("src/b.rs#function:helper", 0.75));
    assert_eq!(edge.to, "src/b.rs#function:helper");
    assert_eq!(edge.confidence, 0.75);
    assert_eq!(edge.provenance, EdgeProvenance::Inferred);
}

#[test]
fn resolution_can_never_reach_full_confidence() {
    let mut edge = SourceEdge::unresolved("a", SourceEdgeKind::Calls, "helper");
    edge.resolve_to("src/b.rs#function:helper", 1.0);
    assert_eq!(edge.confidence, MAX_RESOLVED_INFERRED_CONFIDENCE);
    assert!(edge.confidence < 1.0);
}

#[test]
fn resolution_never_touches_a_parser_edge() {
    let mut edge = SourceEdge::parser("a", "b", SourceEdgeKind::Calls, "helper");
    assert!(!edge.resolve_to("src/elsewhere.rs#function:helper", 0.9));
    assert_eq!(edge.to, "b");
    assert_eq!(edge.confidence, 1.0);
    assert_eq!(edge.provenance, EdgeProvenance::Parser);
}

#[test]
fn resolution_never_retargets_an_already_resolved_edge() {
    let mut edge = SourceEdge::inferred("a", "b", SourceEdgeKind::Calls, "helper", 0.4);
    assert!(!edge.resolve_to("c", 0.9));
    assert_eq!(edge.to, "b");
}

#[test]
fn a_type_and_its_implementation_do_not_collide() {
    // Rust's `struct Widget` + `impl Widget` is the canonical case: same
    // scope, two genuinely distinct nodes.
    let scope = ["Widget".to_string()];
    assert_ne!(
        node_id(Path::new("src/lib.rs"), SourceNodeKind::Type, &scope),
        node_id(
            Path::new("src/lib.rs"),
            SourceNodeKind::Implementation,
            &scope
        )
    );
}

#[test]
fn coverage_reports_whether_symbols_are_expected() {
    assert!(FileCoverage::Full.has_symbols());
    assert!(!FileCoverage::Oversized { bytes: 1, limit: 0 }.has_symbols());
    assert_eq!(
        FileCoverage::LexicalOnly {
            detail: String::new()
        }
        .status(),
        "lexical-only"
    );
}
