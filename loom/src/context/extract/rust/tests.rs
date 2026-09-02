//! Extraction tests for the Rust harness: node ids, edge shape, and what a
//! qualified call is allowed to claim.

use std::path::Path;

use super::*;

const FIXTURE: &str = r#"
mod example {
    use crate::external::Thing;

    struct Widget;

    impl Widget {
        fn call_helper(&self) {
            self.helper();
            Widget::describe();
        }

        fn helper(&self) {}

        fn describe() {}
    }

    fn invoke_external() {
        missing_api();
        crate::other::helper();
        Vec::<u8>::new();
        Vec::<std::string::String>::new();
    }

    trait Describable {}
}
"#;

/// The fixture, extracted as `src/fixture.rs`.
fn extraction() -> FileExtraction {
    RustExtractor::new()
        .extract(Path::new("src/fixture.rs"), FIXTURE.as_bytes())
        .unwrap()
}

/// The one edge naming `symbol`, which every caller here expects to exist.
fn edge_naming(symbol: &str) -> crate::context::source_graph::SourceEdge {
    extraction()
        .edges
        .into_iter()
        .find(|edge| edge.symbol == symbol)
        .unwrap_or_else(|| panic!("no edge naming {symbol} was extracted"))
}

#[test]
fn extracts_the_expected_node_ids() {
    let extraction = extraction();
    let mut ids: Vec<_> = extraction
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    ids.sort_unstable();

    assert_eq!(
        ids,
        vec![
            "src/fixture.rs",
            "src/fixture.rs#function:example::Widget::call_helper",
            "src/fixture.rs#function:example::Widget::describe",
            "src/fixture.rs#function:example::Widget::helper",
            "src/fixture.rs#function:example::invoke_external",
            "src/fixture.rs#implementation:example::Widget",
            "src/fixture.rs#interface:example::Describable",
            "src/fixture.rs#module:example",
            "src/fixture.rs#type:example::Widget",
        ]
    );
}

#[test]
fn a_type_and_its_impl_block_get_distinct_ids() {
    let extraction = extraction();

    let type_id = extraction
        .nodes
        .iter()
        .find(|node| node.kind == SourceNodeKind::Type)
        .map(|node| node.id.as_str())
        .unwrap();
    let implementation_id = extraction
        .nodes
        .iter()
        .find(|node| node.kind == SourceNodeKind::Implementation)
        .map(|node| node.id.as_str())
        .unwrap();

    assert_ne!(type_id, implementation_id);
    assert_eq!(type_id, "src/fixture.rs#type:example::Widget");
    assert_eq!(
        implementation_id,
        "src/fixture.rs#implementation:example::Widget"
    );
}

/// Expected edges as `(from, to, kind, provenance, symbol)`, kept at module
/// scope so the assertion below stays short — the maintainability scanner
/// budgets function bodies, not `const` declarations.
///
/// The symbol is part of the expectation because it is what a qualified call
/// adds: `Widget::describe` and `crate::other::helper` are different edges that
/// would otherwise read identically.
const EXPECTED_EDGES: &[(&str, &str, &str, &str, &str)] = &[
    (
        "src/fixture.rs",
        "<unresolved>",
        "imports",
        "inferred",
        "crate::external::Thing",
    ),
    (
        "src/fixture.rs",
        "src/fixture.rs#module:example",
        "contains",
        "parser",
        "example",
    ),
    (
        "src/fixture.rs#function:example::Widget::call_helper",
        "src/fixture.rs#function:example::Widget::describe",
        "calls",
        "parser",
        "Widget::describe",
    ),
    (
        "src/fixture.rs#function:example::Widget::call_helper",
        "src/fixture.rs#function:example::Widget::helper",
        "calls",
        "parser",
        "helper",
    ),
    (
        "src/fixture.rs#function:example::invoke_external",
        "<unresolved>",
        "calls",
        "inferred",
        "Vec::new",
    ),
    (
        "src/fixture.rs#function:example::invoke_external",
        "<unresolved>",
        "calls",
        "inferred",
        "crate::other::helper",
    ),
    (
        "src/fixture.rs#function:example::invoke_external",
        "<unresolved>",
        "calls",
        "inferred",
        "missing_api",
    ),
    (
        "src/fixture.rs#implementation:example::Widget",
        "src/fixture.rs#function:example::Widget::call_helper",
        "contains",
        "parser",
        "call_helper",
    ),
    (
        "src/fixture.rs#implementation:example::Widget",
        "src/fixture.rs#function:example::Widget::describe",
        "contains",
        "parser",
        "describe",
    ),
    (
        "src/fixture.rs#implementation:example::Widget",
        "src/fixture.rs#function:example::Widget::helper",
        "contains",
        "parser",
        "helper",
    ),
    (
        "src/fixture.rs#module:example",
        "src/fixture.rs#function:example::invoke_external",
        "contains",
        "parser",
        "invoke_external",
    ),
    (
        "src/fixture.rs#module:example",
        "src/fixture.rs#implementation:example::Widget",
        "contains",
        "parser",
        "Widget",
    ),
    (
        "src/fixture.rs#module:example",
        "src/fixture.rs#interface:example::Describable",
        "contains",
        "parser",
        "Describable",
    ),
    (
        "src/fixture.rs#module:example",
        "src/fixture.rs#type:example::Widget",
        "contains",
        "parser",
        "Widget",
    ),
];

#[test]
fn extracts_the_expected_edges() {
    let extraction = extraction();
    let mut edges: Vec<_> = extraction
        .edges
        .iter()
        .map(|edge| {
            (
                edge.from.as_str(),
                edge.to.as_str(),
                edge.kind.as_str(),
                edge.provenance.as_str(),
                edge.symbol.as_str(),
            )
        })
        .collect();
    edges.sort_unstable();

    assert_eq!(edges, EXPECTED_EDGES);
}

/// A call written `Type::assoc()` resolves inside the file when the file holds
/// that `impl`, and the edge the grammar proved keeps the written spelling.
#[test]
fn a_qualified_call_resolves_against_the_impl_it_names() {
    let edge = edge_naming("Widget::describe");

    assert_eq!(edge.to, "src/fixture.rs#function:example::Widget::describe");
    assert_eq!(
        edge.provenance,
        crate::context::source_graph::EdgeProvenance::Parser
    );
}

/// A qualified path leading out of this file stays a guess even when its last
/// segment names something the file does define.
#[test]
fn a_qualified_call_out_of_the_file_never_claims_a_local_namesake() {
    let edge = edge_naming("crate::other::helper");

    assert!(edge.is_unresolved());
    assert_eq!(
        edge.provenance,
        crate::context::source_graph::EdgeProvenance::Inferred
    );
    assert!(edge.confidence <= 0.5);
}

/// A turbofish says what a call is instantiated at, not what it calls, so
/// `Vec::<u8>::new()` is recorded as a call to `Vec::new` — and so is
/// `Vec::<std::string::String>::new()`, whose type argument is itself a path
/// and must not leave `string` behind in the callee.
#[test]
fn a_turbofish_is_dropped_from_the_recorded_callee() {
    let edge = edge_naming("Vec::new");

    assert!(edge.is_unresolved());
    assert_eq!(
        edge.provenance,
        crate::context::source_graph::EdgeProvenance::Inferred
    );
    assert!(edge.confidence <= 0.5);

    let stray: Vec<String> = extraction()
        .edges
        .into_iter()
        .map(|edge| edge.symbol)
        .filter(|symbol| symbol.starts_with("Vec::") && symbol != "Vec::new")
        .collect();
    assert!(
        stray.is_empty(),
        "a type argument leaked into the callee: {stray:?}"
    );
}

#[test]
fn marks_undefined_calls_as_low_confidence_inferred_edges() {
    let edge = edge_naming("missing_api");

    assert_eq!(
        edge.provenance,
        crate::context::source_graph::EdgeProvenance::Inferred
    );
    assert!(edge.confidence <= 0.5);
    assert!(edge.is_unresolved());
}

#[test]
fn syntax_errors_keep_only_the_file_node() {
    let extraction = RustExtractor::new()
        .extract(Path::new("src/broken.rs"), b"fn broken( {")
        .unwrap();

    assert_eq!(extraction.coverage.status(), "parse-error");
    assert_eq!(extraction.nodes.len(), 1);
}
