use super::*;
use std::path::Path;

fn span(start: usize, end: usize) -> Span {
    Span {
        start_byte: start,
        end_byte: end,
        line_start: 1,
        line_end: 1,
    }
}

#[test]
fn syntax_confidence_is_clamped_to_the_ceiling() {
    let edge = SourceEdge::syntax("a", SourceEdgeKind::Calls, "f", span(0, 1), 0.99);
    assert_eq!(edge.confidence, MAX_SYNTAX_CONFIDENCE);
    assert_eq!(edge.provenance, EdgeProvenance::Syntax);
    assert!(edge.is_unresolved());
    assert_eq!(edge.site_count(), 1);
}

#[test]
fn a_structural_edge_is_fully_confident_and_resolved() {
    let edge = SourceEdge::structural("a", "b", "f");
    assert_eq!(edge.kind, SourceEdgeKind::Contains);
    assert_eq!(edge.provenance, EdgeProvenance::Structural);
    assert_eq!(edge.confidence, 1.0);
    assert!(!edge.is_unresolved());
    assert_eq!(edge.site_count(), 0);
}

#[test]
fn a_syntax_edge_names_the_symbol_it_could_not_find() {
    let edge = SourceEdge::syntax(
        "a",
        SourceEdgeKind::Calls,
        "dynamic_target",
        span(4, 18),
        syntax_confidence(SourceEdgeKind::Calls),
    );
    assert!(edge.is_unresolved());
    assert_eq!(edge.symbol, "dynamic_target");
    assert_eq!(edge.confidence, 0.3);
    assert_eq!(edge.sites, vec![span(4, 18)]);
}

#[test]
fn syntax_confidence_depends_on_the_edge_kind() {
    assert_eq!(syntax_confidence(SourceEdgeKind::Calls), 0.3);
    assert_eq!(syntax_confidence(SourceEdgeKind::Imports), 0.5);
    assert_eq!(syntax_confidence(SourceEdgeKind::References), 0.5);
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
fn bound_edges_take_the_provenance_ceiling() {
    let edge = SourceEdge::bound(
        "a",
        "b",
        SourceEdgeKind::Calls,
        "f",
        span(0, 1),
        EdgeProvenance::LocalName,
    );
    assert_eq!(edge.confidence, LOCAL_NAME_CONFIDENCE);
    assert_eq!(edge.to, "b");
    assert_eq!(edge.site_count(), 1);
}

#[test]
fn only_containment_reaches_full_confidence() {
    for provenance in [
        EdgeProvenance::Receiver,
        EdgeProvenance::Import,
        EdgeProvenance::LocalName,
        EdgeProvenance::UniqueName,
        EdgeProvenance::Syntax,
    ] {
        assert!(provenance.ceiling() < 1.0, "{provenance}");
    }
    assert_eq!(EdgeProvenance::Structural.ceiling(), 1.0);
}

#[test]
fn provenance_rank_orders_strongest_first() {
    let mut all = [
        EdgeProvenance::Syntax,
        EdgeProvenance::UniqueName,
        EdgeProvenance::LocalName,
        EdgeProvenance::Import,
        EdgeProvenance::Receiver,
        EdgeProvenance::Compiler,
        EdgeProvenance::Structural,
    ];
    all.sort_by_key(|provenance| std::cmp::Reverse(provenance.rank()));
    let names: Vec<&str> = all.iter().map(|provenance| provenance.as_str()).collect();
    assert_eq!(
        names,
        [
            "structural",
            "compiler",
            "receiver",
            "import",
            "local-name",
            "unique-name",
            "syntax"
        ]
    );
    assert_eq!(EdgeProvenance::Structural.rank(), 6);
    assert_eq!(EdgeProvenance::Syntax.rank(), 0);
}

#[test]
fn binding_raises_an_unresolved_edge_to_the_provenance_ceiling() {
    let mut edge = SourceEdge::syntax("a", SourceEdgeKind::Calls, "helper", span(0, 6), 0.3)
        .with_candidates(vec!["x".to_string(), "y".to_string()]);
    assert!(edge.bind("src/b.rs#function:helper", EdgeProvenance::UniqueName));
    assert_eq!(edge.to, "src/b.rs#function:helper");
    assert_eq!(edge.confidence, UNIQUE_NAME_CONFIDENCE);
    assert_eq!(edge.provenance, EdgeProvenance::UniqueName);
    assert!(edge.candidates.is_empty());

    let mut import = SourceEdge::syntax("a", SourceEdgeKind::Calls, "helper", span(0, 6), 0.3);
    assert!(import.bind("t", EdgeProvenance::Import));
    assert_eq!(import.confidence, IMPORT_CONFIDENCE);
}

#[test]
fn bind_refuses_a_structural_edge() {
    let mut edge = SourceEdge::structural("a", "b", "helper");
    assert!(!edge.bind("src/elsewhere.rs#function:helper", EdgeProvenance::Import));
    assert_eq!(edge.to, "b");
    assert_eq!(edge.confidence, 1.0);
    assert_eq!(edge.provenance, EdgeProvenance::Structural);
}

#[test]
fn bind_refuses_an_already_bound_edge() {
    let mut edge = SourceEdge::bound(
        "a",
        "b",
        SourceEdgeKind::Calls,
        "helper",
        span(0, 6),
        EdgeProvenance::LocalName,
    );
    assert!(!edge.bind("c", EdgeProvenance::Import));
    assert_eq!(edge.to, "b");
    assert_eq!(edge.provenance, EdgeProvenance::LocalName);
}

#[test]
fn bind_refuses_provenances_only_extraction_can_prove() {
    let mut edge = SourceEdge::syntax("a", SourceEdgeKind::Calls, "helper", span(0, 6), 0.3);
    for provenance in [
        EdgeProvenance::LocalName,
        EdgeProvenance::Structural,
        EdgeProvenance::Syntax,
        EdgeProvenance::Compiler,
    ] {
        assert!(!edge.bind("c", provenance), "{provenance}");
    }
    assert!(edge.is_unresolved());
    assert_eq!(edge.provenance, EdgeProvenance::Syntax);
}

#[test]
fn unbind_restores_the_extraction_time_state() {
    let mut calls = SourceEdge::syntax("a", SourceEdgeKind::Calls, "helper", span(0, 6), 0.3);
    assert!(calls.bind("b", EdgeProvenance::Receiver));
    calls.unbind();
    assert!(calls.is_unresolved());
    assert_eq!(calls.provenance, EdgeProvenance::Syntax);
    assert_eq!(calls.confidence, 0.3);
    assert!(calls.candidates.is_empty());

    let mut import = SourceEdge::syntax("a", SourceEdgeKind::Imports, "x", span(0, 1), 0.5);
    assert!(import.bind("b", EdgeProvenance::Import));
    import.unbind();
    assert_eq!(import.confidence, 0.5);
}

#[test]
fn site_ids_name_the_path_and_byte_range() {
    assert_eq!(site_id("src/lib.rs", &span(10, 24)), "src/lib.rs@10-24");
}

fn import(path: &str, name: Option<&str>, alias: Option<&str>, glob: bool) -> ImportBinding {
    ImportBinding {
        path: path.to_string(),
        name: name.map(str::to_string),
        alias: alias.map(str::to_string),
        glob,
        exported_as: None,
        site: Span::default(),
    }
}

#[test]
fn local_name_prefers_alias_then_name_then_last_segment() {
    assert_eq!(
        import("a::b", Some("parse"), Some("p"), false).local_name(),
        Some("p")
    );
    assert_eq!(
        import("a::b", Some("parse"), None, false).local_name(),
        Some("parse")
    );
    assert_eq!(import("a::b::c", None, None, false).local_name(), Some("c"));
    assert_eq!(import("a/b", None, None, false).local_name(), Some("b"));
    assert_eq!(
        import("a.b", None, Some("a.b"), false).local_name(),
        Some("a.b")
    );
}

#[test]
fn local_name_is_none_for_a_glob_and_a_side_effect_import() {
    assert_eq!(import("a::b", None, None, true).local_name(), None);
    assert_eq!(import("x", None, Some(""), false).local_name(), None);
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

#[test]
fn body_hash_is_the_prefixed_sha256_of_the_bytes() {
    assert_eq!(
        body_hash(b"abc"),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
