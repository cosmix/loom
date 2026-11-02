//! What the Java extractor emits: ids, scopes, import bindings and call edges.

use std::path::Path;

use super::*;
use crate::context::source_graph::{EdgeProvenance, FileCoverage, SourceEdge, SourceEdgeKind};

const WIDGET: &str = include_str!("../../../../tests/fixtures/source/java/Widget.java");
const BROKEN: &str = include_str!("../../../../tests/fixtures/source/java/syntax_error.java");
const PATH: &str = "src/Widget.java";

fn extract(source: &str) -> FileExtraction {
    JavaExtractor::new()
        .extract(Path::new(PATH), source.as_bytes())
        .unwrap()
}

/// The `Calls` edges naming `symbol`, in the extraction's canonical order.
fn calls<'a>(extraction: &'a FileExtraction, symbol: &str) -> Vec<&'a SourceEdge> {
    extraction
        .edges
        .iter()
        .filter(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == symbol)
        .collect()
}

fn id(suffix: &str) -> String {
    format!("{PATH}#{suffix}")
}

#[test]
fn declarations_get_scoped_ids_and_the_package_is_a_module() {
    let extraction = extract(WIDGET);
    let mut ids: Vec<&str> = extraction
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|id| !id.contains("::step"))
        .collect();
    ids.sort_unstable();

    let mut expected = vec![
        PATH.to_string(),
        id("module:app.core"),
        id("type:Widget"),
        id("function:Widget::run"),
        id("function:Widget::ping"),
        id("type:Widget::Inner"),
        id("function:Widget::Inner::pong"),
    ];
    expected.sort_unstable();
    assert_eq!(ids, expected);
    assert_eq!(extraction.coverage, FileCoverage::Full);

    let module = extraction
        .nodes
        .iter()
        .find(|node| node.kind == SourceNodeKind::Module)
        .unwrap();
    assert_eq!(module.scope, vec!["app.core".to_string()]);
}

#[test]
fn overloads_get_distinct_ids_sharing_one_symbol_key() {
    let extraction = extract(WIDGET);
    let steps: Vec<_> = extraction
        .nodes
        .iter()
        .filter(|node| node.kind == SourceNodeKind::Function && node.scope == ["Widget", "step"])
        .collect();

    assert_eq!(steps.len(), 2);
    assert_ne!(steps[0].id, steps[1].id);
    for step in steps {
        assert_eq!(step.symbol_key, id("function:Widget::step"));
        let suffix = step
            .id
            .strip_prefix(&format!("{}@", step.symbol_key))
            .unwrap();
        assert_eq!(suffix.len(), 8, "id: {}", step.id);
    }
}

#[test]
fn a_this_call_binds_to_the_one_member_and_keeps_overloads_as_candidates() {
    let extraction = extract(WIDGET);

    let ping = calls(&extraction, "ping");
    assert_eq!(ping.len(), 2);
    let via_this = ping.iter().find(|edge| edge.receiver.is_some()).unwrap();
    assert_eq!(via_this.from, id("function:Widget::run"));
    assert_eq!(via_this.to, id("function:Widget::ping"));
    assert_eq!(via_this.provenance, EdgeProvenance::Receiver);
    assert_eq!(via_this.receiver.as_deref(), Some("this"));

    let step = calls(&extraction, "step");
    assert_eq!(step.len(), 1);
    assert!(step[0].is_unresolved());
    assert_eq!(step[0].provenance, EdgeProvenance::Syntax);
    assert_eq!(step[0].candidates.len(), 2, "edge: {:#?}", step[0]);
    assert!(step[0]
        .candidates
        .iter()
        .all(|c| c.starts_with(&id("function:Widget::step@"))));
}

#[test]
fn a_bare_call_reaches_a_member_of_an_outer_class() {
    let extraction = extract(WIDGET);
    let bare = calls(&extraction, "ping")
        .into_iter()
        .find(|edge| edge.receiver.is_none())
        .unwrap();

    assert_eq!(bare.from, id("function:Widget::Inner::pong"));
    assert_eq!(bare.to, id("function:Widget::ping"));
    assert_eq!(bare.provenance, EdgeProvenance::LocalName);

    let recursive = calls(&extraction, "pong");
    assert_eq!(recursive.len(), 1);
    assert_eq!(recursive[0].to, id("function:Widget::Inner::pong"));
    assert_eq!(recursive[0].provenance, EdgeProvenance::Receiver);
}

#[test]
fn a_dynamic_receiver_stays_unresolved_without_retaining_expression_text() {
    let extraction = extract(WIDGET);

    for (symbol, receiver) in [("assist", "helper"), ("go", "")] {
        let edges = calls(&extraction, symbol);
        let member: Vec<_> = edges.iter().filter(|e| e.receiver.is_some()).collect();
        assert_eq!(member.len(), 1, "symbol {symbol}: {edges:#?}");
        assert!(member[0].is_unresolved());
        assert_eq!(member[0].receiver.as_deref(), Some(receiver));
    }
}

#[test]
fn a_constructor_call_names_the_class() {
    let extraction = extract(WIDGET);
    let created = calls(&extraction, "Widget");

    assert_eq!(created.len(), 1);
    assert_eq!(created[0].from, id("function:Widget::run"));
    assert_eq!(created[0].to, id("type:Widget"));
    assert_eq!(created[0].receiver, None);
}

#[test]
fn a_statically_imported_name_is_left_to_the_resolver() {
    let extraction = extract(WIDGET);
    let bare: Vec<_> = calls(&extraction, "assist")
        .into_iter()
        .filter(|edge| edge.receiver.is_none())
        .collect();

    assert_eq!(bare.len(), 1);
    assert!(bare[0].is_unresolved());
    assert!(bare[0].candidates.is_empty());
}

#[test]
fn imports_bind_their_last_segment_and_a_star_binds_the_package() {
    let extraction = extract(WIDGET);
    let bindings: Vec<_> = extraction
        .imports
        .iter()
        .map(|b| {
            (
                b.path.as_str(),
                b.name.as_deref(),
                b.alias.as_deref(),
                b.glob,
            )
        })
        .collect();

    assert_eq!(
        bindings,
        vec![
            ("java.util.List", Some("List"), None, false),
            ("java.util", None, None, true),
            ("app.util.Helpers.assist", Some("assist"), None, false),
        ]
    );

    let imported: Vec<_> = extraction
        .edges
        .iter()
        .filter(|edge| edge.kind == SourceEdgeKind::Imports)
        .map(|edge| edge.symbol.as_str())
        .collect();
    assert_eq!(
        imported,
        vec!["app.util.Helpers.assist", "java.util", "java.util.List"]
    );
}

#[test]
fn a_static_star_import_is_a_glob_of_the_class() {
    let extraction = extract("import static a.b.C.*;\nclass W {}\n");

    assert_eq!(extraction.imports.len(), 1);
    assert_eq!(extraction.imports[0].path, "a.b.C");
    assert!(extraction.imports[0].glob);
    assert_eq!(extraction.imports[0].name, None);
}

#[test]
fn generic_invocations_and_creations_name_the_method_and_the_class() {
    let extraction =
        extract("class A {\n  void f() {\n    x.<String>m();\n    new B<String>();\n  }\n}\n");

    let method = calls(&extraction, "m");
    assert_eq!(method.len(), 1);
    assert_eq!(method[0].receiver.as_deref(), Some("x"));
    let created = calls(&extraction, "B");
    assert_eq!(created.len(), 1);
    assert!(created[0].is_unresolved());
}

#[test]
fn super_is_not_a_self_receiver() {
    let extraction = extract("class A {\n  void f() {\n    super.g();\n  }\n  void g() {}\n}\n");
    let edge = &calls(&extraction, "g")[0];

    assert_eq!(edge.receiver.as_deref(), Some("super"));
    assert!(edge.is_unresolved());
}

#[test]
fn a_method_without_a_body_is_not_a_definition() {
    let extraction = extract("interface I {\n  void a();\n  default void b() {}\n}\n");
    let mut ids: Vec<&str> = extraction.nodes.iter().map(|n| n.id.as_str()).collect();
    ids.sort_unstable();

    let mut expected = vec![PATH.to_string(), id("interface:I"), id("function:I::b")];
    expected.sort_unstable();
    assert_eq!(ids, expected);
}

#[test]
fn a_syntax_error_keeps_only_the_file_node() {
    let extraction = extract(BROKEN);

    assert_eq!(extraction.coverage.status(), "parse-error");
    assert_eq!(extraction.nodes.len(), 1);
}
