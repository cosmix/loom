//! TSX extraction: TypeScript declarations, JavaScript-shaped function values and JSX
//! element references.

use crate::context::extract::javascript::tests::{
    bindings_for, extract, node_ids, references_to, Want,
};
use crate::context::extract::FileExtraction;
use crate::context::source_graph::{
    EdgeProvenance, FileCoverage, SourceEdgeKind, UNRESOLVED_TARGET,
};

use crate::context::extract::javascript::JavaScriptExtractor;
use crate::context::extract::typescript::TypeScriptExtractor;
use crate::context::source_graph::{SourceNode, SourceNodeKind};

use super::TsxExtractor;

const COMPONENT: &str = include_str!("../../../../tests/fixtures/source/tsx/component.tsx");
const SYNTAX_ERROR: &str = include_str!("../../../../tests/fixtures/source/tsx/syntax_error.tsx");

fn tsx(source: &str) -> FileExtraction {
    extract(&TsxExtractor::new(), "src/component.tsx", source)
}

/// A `Calls` edge between two ids of `src/component.tsx`, each given by its fragment.
fn call(from: &str, to: &str, symbol: &str, provenance: EdgeProvenance, line: usize) -> Want {
    let id = |fragment: &str| match fragment {
        UNRESOLVED_TARGET => fragment.to_string(),
        _ => format!("src/component.tsx#{fragment}"),
    };
    Want::new(
        SourceEdgeKind::Calls,
        (&id(from), &id(to)),
        symbol,
        provenance,
        line,
    )
}

#[test]
fn component_fixture_yields_declarations_with_full_coverage() {
    let extraction = tsx(COMPONENT);

    assert_eq!(extraction.coverage, FileCoverage::Full);
    assert_eq!(
        node_ids(&extraction),
        vec![
            "src/component.tsx",
            "src/component.tsx#function:App",
            "src/component.tsx#function:Other::run",
            "src/component.tsx#function:Screen::draw",
            "src/component.tsx#function:Screen::run",
            "src/component.tsx#function:helper",
            "src/component.tsx#interface:Props",
            "src/component.tsx#type:Other",
            "src/component.tsx#type:Screen",
        ]
    );
}

#[test]
fn jsx_components_become_reference_edges_and_intrinsic_tags_do_not() {
    let extraction = tsx(COMPONENT);

    // `Button` and `Panel` are imported, `ui.Icon` names its last segment.
    for (symbol, line) in [("Button", 12), ("Panel", 13), ("Icon", 14)] {
        Want::new(
            SourceEdgeKind::References,
            ("src/component.tsx#function:App", UNRESOLVED_TARGET),
            symbol,
            EdgeProvenance::Syntax,
            line,
        )
        .check(&extraction);
    }
    assert_eq!(references_to(&extraction, "div"), 0);
}

#[test]
fn this_calls_bind_within_each_class_even_with_duplicate_method_names() {
    let extraction = tsx(COMPONENT);

    call(
        "function:Screen::run",
        "function:Screen::draw",
        "draw",
        EdgeProvenance::Receiver,
        22,
    )
    .on("this")
    .check(&extraction);
    call(
        "function:Other::run",
        "function:Other::run",
        "run",
        EdgeProvenance::Receiver,
        31,
    )
    .on("this")
    .check(&extraction);
    call(
        "function:Screen::run",
        "function:helper",
        "helper",
        EdgeProvenance::LocalName,
        23,
    )
    .check(&extraction);
}

#[test]
fn dynamic_receivers_stay_unresolved() {
    let extraction = tsx(COMPONENT);

    call(
        "function:Other::run",
        UNRESOLVED_TARGET,
        "run",
        EdgeProvenance::Syntax,
        32,
    )
    .on("registry")
    .check(&extraction);
}

#[test]
fn imports_carry_bindings() {
    let extraction = tsx(COMPONENT);

    assert_eq!(
        bindings_for(&extraction, "./Button"),
        vec![(Some("Button"), Some("Button"), false)]
    );
    assert_eq!(
        bindings_for(&extraction, "./Card"),
        vec![(Some("Card"), Some("Panel"), false)]
    );
    assert_eq!(
        bindings_for(&extraction, "./shared"),
        vec![(None, None, true)]
    );
}

#[test]
fn syntax_errors_keep_only_the_file_node() {
    let extraction = tsx(SYNTAX_ERROR);

    assert_eq!(extraction.coverage.status(), "parse-error");
    assert_eq!(extraction.nodes.len(), 1);
    assert!(extraction.edges.is_empty());
}

/// The node with `id` in `extraction`; panics when absent.
fn node<'a>(extraction: &'a FileExtraction, id: &str) -> &'a SourceNode {
    extraction
        .nodes
        .iter()
        .find(|n| n.id == id)
        .unwrap_or_else(|| panic!("no node {id}: {:#?}", extraction.nodes))
}

#[test]
fn arrow_const_spans_the_declarator_like_javascript() {
    let source = "\nexport const App = () => <div />;\n";
    let tsx_extraction = tsx(source);
    let js_extraction = extract(&JavaScriptExtractor::new(), "src/component.jsx", source);

    let in_tsx = node(&tsx_extraction, "src/component.tsx#function:App");
    let in_js = node(&js_extraction, "src/component.jsx#function:App");

    assert_eq!(in_tsx.kind, SourceNodeKind::Function);
    assert_eq!(in_tsx.span, in_js.span);
    assert_eq!(in_tsx.span.line_start, 2);
    assert_eq!(in_tsx.signature, in_js.signature);
    assert!(
        in_tsx.signature.starts_with("App"),
        "signature: {}",
        in_tsx.signature
    );
}

#[test]
fn sibling_function_values_own_their_calls_in_all_ecmascript_dialects() {
    let source = "export const first = () => alpha(), second = () => beta();\n\
export const third = function () { gamma(); }, fourth = function () { delta(); };\n\
function outer() {\n\
  const innerFirst = () => one(), innerSecond = () => two();\n\
  var innerThird = function () { three(); }, innerFourth = function () { four(); };\n\
  function nested() { deep(); }\n\
}\n";
    let extractors: [(&dyn crate::context::extract::SourceGraphExtractor, &str); 3] = [
        (&JavaScriptExtractor::new(), "src/test.js"),
        (&TsxExtractor::new(), "src/test.tsx"),
        (&TypeScriptExtractor::new(), "src/test.ts"),
    ];
    for (extractor, path) in extractors {
        let result = extract(extractor, path, source);
        assert_sibling_calls(&result, path);
    }
}

fn assert_sibling_calls(result: &FileExtraction, path: &str) {
    assert_eq!(result.coverage, FileCoverage::Full);
    for (scope, symbol) in [
        ("first", "alpha"),
        ("second", "beta"),
        ("third", "gamma"),
        ("fourth", "delta"),
        ("outer::innerFirst", "one"),
        ("outer::innerSecond", "two"),
        ("outer::innerThird", "three"),
        ("outer::innerFourth", "four"),
        ("outer::nested", "deep"),
    ] {
        let id = format!("{path}#function:{scope}");
        assert!(
            result.nodes.iter().any(|n| n.id == id),
            "missing {id}: {:#?}",
            result.nodes
        );
        assert!(
            result
                .edges
                .iter()
                .any(|e| e.from == id && e.kind == SourceEdgeKind::Calls && e.symbol == symbol),
            "missing {id} -> {symbol}: {:#?}",
            result.edges
        );
        let parent = if scope.starts_with("outer::") {
            format!("{path}#function:outer")
        } else {
            path.to_string()
        };
        assert!(
            result
                .edges
                .iter()
                .any(|e| e.from == parent && e.to == id && e.kind == SourceEdgeKind::Contains),
            "missing containment {parent} -> {id}"
        );
    }
    assert_eq!(
        result
            .nodes
            .iter()
            .filter(|n| n.kind == SourceNodeKind::Function)
            .count(),
        10
    );
}

#[test]
fn function_expression_const_is_a_function() {
    let extraction = tsx("export const f = function () {};\n");

    assert_eq!(
        node(&extraction, "src/component.tsx#function:f").kind,
        SourceNodeKind::Function
    );
    assert_eq!(
        node_ids(&extraction),
        vec!["src/component.tsx", "src/component.tsx#function:f"]
    );
}

#[test]
fn generator_declaration_is_a_function() {
    let extraction = tsx("function* g() {}\n");

    assert_eq!(
        node(&extraction, "src/component.tsx#function:g").kind,
        SourceNodeKind::Function
    );
}

#[test]
fn typescript_declarations_and_plain_constants_survive() {
    let extraction = tsx(
        "export const LIMIT = 3;\nenum Color { Red }\nnamespace Ns { }\n\
         abstract class Base { }\ntype Id = string;\n",
    );

    assert_eq!(
        node_ids(&extraction),
        vec![
            "src/component.tsx",
            "src/component.tsx#constant:LIMIT",
            "src/component.tsx#module:Ns",
            "src/component.tsx#type:Base",
            "src/component.tsx#type:Color",
            "src/component.tsx#type:Id",
        ]
    );
}
