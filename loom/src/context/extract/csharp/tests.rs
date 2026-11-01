//! What the C# extractor emits: ids, scopes, using bindings and call edges.

use std::path::Path;

use super::*;
use crate::context::source_graph::{EdgeProvenance, FileCoverage, SourceEdge, SourceEdgeKind};

const WIDGET: &str = include_str!("../../../../tests/fixtures/source/csharp/Widget.cs");
const BROKEN: &str = include_str!("../../../../tests/fixtures/source/csharp/syntax_error.cs");
const PATH: &str = "src/Widget.cs";

fn extract(source: &str) -> FileExtraction {
    CSharpExtractor::new()
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

fn sorted_ids(extraction: &FileExtraction) -> Vec<&str> {
    let mut ids: Vec<&str> = extraction.nodes.iter().map(|n| n.id.as_str()).collect();
    ids.sort_unstable();
    ids
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort_unstable();
    ids
}

#[test]
fn a_block_namespace_is_a_module_that_scopes_its_types() {
    let extraction = extract(WIDGET);
    let ids: Vec<&str> = sorted_ids(&extraction)
        .into_iter()
        .filter(|id| !id.contains("::Step"))
        .collect();

    let expected = sorted(vec![
        PATH.to_string(),
        id("module:App.Core"),
        id("type:App.Core::Widget"),
        id("function:App.Core::Widget::Run"),
        id("function:App.Core::Widget::Ping"),
        id("type:App.Core::Widget::Inner"),
        id("function:App.Core::Widget::Inner::Pong"),
    ]);
    assert_eq!(ids, expected);
    assert_eq!(extraction.coverage, FileCoverage::Full);

    let module = extraction
        .nodes
        .iter()
        .find(|node| node.kind == SourceNodeKind::Module)
        .unwrap();
    assert_eq!(module.scope, vec!["App.Core".to_string()]);
}

/// A file-scoped namespace does not enclose the file's types, so it is a
/// module node with one dotted segment and the types keep no namespace scope.
#[test]
fn a_file_scoped_namespace_is_a_module_without_scoping_its_types() {
    let extraction =
        extract("namespace A.B;\n\npublic class Widget\n{\n    public void Run() { }\n}\n");

    let expected = sorted(vec![
        PATH.to_string(),
        id("module:A.B"),
        id("type:Widget"),
        id("function:Widget::Run"),
    ]);
    assert_eq!(sorted_ids(&extraction), expected);
    let module = extraction
        .nodes
        .iter()
        .find(|node| node.kind == SourceNodeKind::Module)
        .unwrap();
    assert_eq!(module.scope, vec!["A.B".to_string()]);
    assert_eq!(extraction.coverage, FileCoverage::Full);
}

#[test]
fn overloads_get_distinct_ids_sharing_one_symbol_key() {
    let extraction = extract(WIDGET);
    let steps: Vec<_> = extraction
        .nodes
        .iter()
        .filter(|node| {
            node.kind == SourceNodeKind::Function && node.scope.ends_with(&["Step".to_string()])
        })
        .collect();

    assert_eq!(steps.len(), 2);
    assert_ne!(steps[0].id, steps[1].id);
    for step in steps {
        assert_eq!(step.symbol_key, id("function:App.Core::Widget::Step"));
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

    let ping = calls(&extraction, "Ping");
    let via_this = ping.iter().find(|edge| edge.receiver.is_some()).unwrap();
    assert_eq!(via_this.from, id("function:App.Core::Widget::Run"));
    assert_eq!(via_this.to, id("function:App.Core::Widget::Ping"));
    assert_eq!(via_this.provenance, EdgeProvenance::Receiver);
    assert_eq!(via_this.receiver.as_deref(), Some("this"));

    let step = calls(&extraction, "Step");
    assert_eq!(step.len(), 1);
    assert!(step[0].is_unresolved());
    assert_eq!(step[0].candidates.len(), 2, "edge: {:#?}", step[0]);
}

#[test]
fn a_bare_call_reaches_a_member_of_an_outer_class() {
    let extraction = extract(WIDGET);
    let bare = calls(&extraction, "Ping")
        .into_iter()
        .find(|edge| edge.receiver.is_none())
        .unwrap();

    assert_eq!(bare.from, id("function:App.Core::Widget::Inner::Pong"));
    assert_eq!(bare.to, id("function:App.Core::Widget::Ping"));
    assert_eq!(bare.provenance, EdgeProvenance::LocalName);
}

#[test]
fn a_dynamic_receiver_stays_unresolved_without_retaining_expression_text() {
    let extraction = extract(WIDGET);

    for (symbol, receiver) in [("Assist", "helper"), ("Go", "")] {
        let edges = calls(&extraction, symbol);
        let member: Vec<_> = edges.iter().filter(|e| e.receiver.is_some()).collect();
        assert_eq!(member.len(), 1, "symbol {symbol}: {edges:#?}");
        assert!(member[0].is_unresolved());
        assert_eq!(member[0].receiver.as_deref(), Some(receiver));
    }
}

#[test]
fn generic_calls_capture_the_identifier_not_the_type_arguments() {
    let extraction = extract(WIDGET);

    let bare = calls(&extraction, "Convert");
    assert_eq!(bare.len(), 1);
    assert_eq!(bare[0].receiver, None);
    assert!(bare[0].is_unresolved());

    let member = calls(&extraction, "Cast");
    assert_eq!(member.len(), 1);
    assert_eq!(member[0].receiver.as_deref(), Some("helper"));
    assert!(calls(&extraction, "Convert<int>").is_empty());
}

#[test]
fn a_constructor_call_names_the_class() {
    let extraction = extract(WIDGET);
    let created = calls(&extraction, "Widget");

    assert_eq!(created.len(), 1);
    assert_eq!(created[0].from, id("function:App.Core::Widget::Run"));
    assert_eq!(created[0].to, id("type:App.Core::Widget"));
    assert_eq!(created[0].provenance, EdgeProvenance::LocalName);
}

#[test]
fn usings_bind_globs_and_an_alias_binds_its_target() {
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
            ("System", None, None, true),
            ("System.Collections.Generic", None, None, true),
            ("System.Math", None, None, true),
            ("App.Core.Helper", Some("Helper"), Some("Alias"), false),
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
        vec![
            "App.Core.Helper",
            "System",
            "System.Collections.Generic",
            "System.Math"
        ]
    );
}

#[test]
fn an_alias_of_a_single_identifier_takes_the_target_as_its_path() {
    let extraction = extract("using Num = Foo;\nclass W { }\n");

    assert_eq!(extraction.imports.len(), 1);
    let binding = &extraction.imports[0];
    assert_eq!(binding.path, "Foo");
    assert_eq!(binding.name.as_deref(), Some("Foo"));
    assert_eq!(binding.alias.as_deref(), Some("Num"));
    assert!(!binding.glob);
}

#[test]
fn a_conditional_call_keeps_its_receiver() {
    let extraction = extract("class A\n{\n    void F(A b) { b?.M(); }\n}\n");
    let edge = &calls(&extraction, "M")[0];

    assert_eq!(edge.receiver.as_deref(), Some("b"));
    assert!(edge.is_unresolved());
}

#[test]
fn a_local_function_is_scoped_under_its_method() {
    let extraction =
        extract("class A\n{\n    void F()\n    {\n        G();\n        void G() { }\n    }\n}\n");

    let call = &calls(&extraction, "G")[0];
    assert_eq!(call.from, id("function:A::F"));
    assert_eq!(call.to, id("function:A::F::G"));
    assert_eq!(call.provenance, EdgeProvenance::LocalName);
}

#[test]
fn base_is_not_a_self_receiver() {
    let extraction = extract("class A\n{\n    void F() { base.G(); }\n    void G() { }\n}\n");
    let edge = &calls(&extraction, "G")[0];

    assert_eq!(edge.receiver.as_deref(), Some("base"));
    assert!(edge.is_unresolved());
}

#[test]
fn a_method_without_a_body_is_not_a_definition() {
    let extraction = extract("interface I\n{\n    void A();\n    void B() { }\n}\n");

    let expected = sorted(vec![
        PATH.to_string(),
        id("interface:I"),
        id("function:I::B"),
    ]);
    assert_eq!(sorted_ids(&extraction), expected);
}

#[test]
fn a_syntax_error_keeps_only_the_file_node() {
    let extraction = extract(BROKEN);

    assert_eq!(extraction.coverage.status(), "parse-error");
    assert_eq!(extraction.nodes.len(), 1);
}
