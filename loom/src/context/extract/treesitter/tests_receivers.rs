//! A self receiver no type encloses: unbound everywhere but Ruby, whose
//! top-level `self` is the `main` object owning the file's top-level methods.

use std::path::Path;

use super::tests::{function, rust};
use crate::context::extract::javascript::JavaScriptExtractor;
use crate::context::extract::python::PythonExtractor;
use crate::context::extract::{FileExtraction, SourceGraphExtractor};
use crate::context::graph_store::{FileEntry, ResolvedGraph};
use crate::context::resolve::resolve_graph;
use crate::context::source_graph::{EdgeProvenance, SourceEdge, SourceEdgeKind};

/// The single `Calls` edge for `symbol`.
fn call<'a>(extraction: &'a FileExtraction, symbol: &str) -> &'a SourceEdge {
    let calls: Vec<_> = extraction
        .edges
        .iter()
        .filter(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == symbol)
        .collect();
    assert_eq!(calls.len(), 1, "calls to {symbol}: {:#?}", extraction.edges);
    calls[0]
}

/// Unbound, with no candidates and the receiver kept.
fn assert_unbound_on(edge: &SourceEdge, receiver: &str) {
    assert_eq!(edge.provenance, EdgeProvenance::Syntax, "edge: {edge:#?}");
    assert!(edge.is_unresolved(), "edge: {edge:#?}");
    assert!(edge.candidates.is_empty(), "edge: {edge:#?}");
    assert_eq!(edge.receiver.as_deref(), Some(receiver), "edge: {edge:#?}");
}

fn assert_shadowed_on(edge: &SourceEdge, owner: &str) {
    assert_eq!(edge.provenance, EdgeProvenance::Syntax, "edge: {edge:#?}");
    assert!(edge.is_unresolved(), "edge: {edge:#?}");
    assert_eq!(edge.receiver.as_deref(), Some("this"), "edge: {edge:#?}");
    assert_eq!(edge.candidates.len(), 1, "edge: {edge:#?}");
    assert!(
        edge.candidates[0].contains(&format!("{owner}::{}", edge.symbol)),
        "edge: {edge:#?}"
    );
}

const ECMASCRIPT_RECEIVER_SOURCE: &[u8] = br#"
class C {
  m() {
    function f() { this.ordinary(); const a = () => this.nestedArrow(); }
    function* gen() { this.generator(); }
    const a = () => this.arrow();
    const g = function() { this.expression(); };
    const h = function*() { this.generatorExpression(); };
    const o = { [this.objectComputed()]() {}, runObject() { this.objectMethod(); const a = () => this.objectArrow(); } };
    const k = class extends this.expressionHeritage() {
      [this.classComputed()]() {}
      runClass() { this.classExpression(); const a = () => this.classArrow(); }
    };
    class Inner extends this.declaredHeritage() {
      [this.declaredComputed()]() {}
      m() { this.inner(); }
      inner() {}
    }
    this.direct();
  }
  ordinary() {} nestedArrow() {} generator() {} arrow() {}
  expression() {} generatorExpression() {} objectMethod() {} objectArrow() {}
  classExpression() {} classArrow() {} direct() {}
  objectComputed() {} expressionHeritage() {} classComputed() {}
  declaredHeritage() {} declaredComputed() {}
}
"#;

const SHADOWED_RECEIVER_CALLS: &[&str] = &[
    "ordinary",
    "nestedArrow",
    "generator",
    "expression",
    "generatorExpression",
    "objectMethod",
    "objectArrow",
    "classExpression",
    "classArrow",
];

const LEXICAL_RECEIVER_CALLS: &[&str] = &[
    "arrow",
    "direct",
    "objectComputed",
    "expressionHeritage",
    "classComputed",
    "declaredHeritage",
    "declaredComputed",
];

#[test]
fn a_self_call_outside_a_type_never_binds_a_free_function() {
    let extraction = rust("fn run() {}\nfn attach() { self.run(); }\n");

    let edge = call(&extraction, "run");
    assert_eq!(edge.from, function("attach"));
    assert_unbound_on(edge, "self");
}

#[test]
fn a_python_self_call_in_a_module_function_stays_unbound() {
    let extraction = PythonExtractor::new()
        .extract(
            Path::new("pkg/m.py"),
            b"def run():\n    pass\n\ndef attach(self):\n    self.run()\n",
        )
        .unwrap();

    assert_unbound_on(call(&extraction, "run"), "self");
}

#[test]
fn a_this_call_in_a_javascript_function_stays_unbound() {
    let extraction = JavaScriptExtractor::new()
        .extract(
            Path::new("src/w.js"),
            b"function render() {}\nfunction Widget() {\n  this.render();\n}\n",
        )
        .unwrap();

    assert_unbound_on(call(&extraction, "render"), "this");
}

#[test]
fn deep_dynamic_receivers_remain_bounded_and_unresolved() {
    let source = format!("function f() {{ root{}; }}", ".step()".repeat(300));
    let extraction = JavaScriptExtractor::new()
        .extract(Path::new("src/w.js"), source.as_bytes())
        .unwrap();
    let calls: Vec<_> = extraction
        .edges
        .iter()
        .filter(|edge| edge.kind == SourceEdgeKind::Calls)
        .collect();
    assert!(!calls.is_empty());
    assert!(calls.iter().all(|edge| edge.is_unresolved()));
    assert!(
        calls
            .iter()
            .filter_map(|edge| edge.receiver.as_ref())
            .map(String::len)
            .sum::<usize>()
            <= 256
    );
}

#[test]
fn long_identifier_receiver_keeps_its_spelling() {
    let name = "a".repeat(300);
    let source = format!("function f() {{ {name}.run(); }}");
    let extraction = JavaScriptExtractor::new()
        .extract(Path::new("src/w.js"), source.as_bytes())
        .unwrap();
    assert_unbound_on(call(&extraction, "run"), &name);

    let path = format!("{name}.nested");
    let source = format!("function f() {{ {path}.run(); }}");
    let extraction = JavaScriptExtractor::new()
        .extract(Path::new("src/w.js"), source.as_bytes())
        .unwrap();
    assert_unbound_on(call(&extraction, "run"), &path);
}

#[test]
fn ecmascript_receiver_boundaries_follow_function_syntax() {
    use crate::context::extract::tsx::TsxExtractor;
    use crate::context::extract::typescript::TypeScriptExtractor;

    let extractors: &[(&dyn SourceGraphExtractor, &str)] = &[
        (&JavaScriptExtractor::new(), "src/w.js"),
        (&TypeScriptExtractor::new(), "src/w.ts"),
        (&TsxExtractor::new(), "src/w.tsx"),
    ];
    for (extractor, path) in extractors {
        let extraction = extractor
            .extract(Path::new(path), ECMASCRIPT_RECEIVER_SOURCE)
            .unwrap();
        for &symbol in SHADOWED_RECEIVER_CALLS {
            assert_shadowed_on(call(&extraction, symbol), "C");
        }
        for &symbol in LEXICAL_RECEIVER_CALLS {
            let edge = call(&extraction, symbol);
            assert_eq!(
                edge.provenance,
                EdgeProvenance::Receiver,
                "{path}: {edge:#?}"
            );
            assert!(
                edge.to.contains(&format!("C::{symbol}")),
                "{path}: {edge:#?}"
            );
            if matches!(symbol, "declaredHeritage" | "declaredComputed") {
                assert_eq!(edge.from, format!("{path}#function:C::m"));
            }
        }
        let inner = call(&extraction, "inner");
        assert_eq!(
            inner.provenance,
            EdgeProvenance::Receiver,
            "{path}: {inner:#?}"
        );
        assert!(
            inner.to.contains("C::m::Inner::inner"),
            "{path}: {inner:#?}"
        );
    }
}

#[test]
fn ecmascript_shadowed_receivers_stay_unresolved_after_graph_resolution() {
    use crate::context::extract::tsx::TsxExtractor;
    use crate::context::extract::typescript::TypeScriptExtractor;

    let extractors: &[(&dyn SourceGraphExtractor, &str)] = &[
        (&JavaScriptExtractor::new(), "src/w.js"),
        (&TypeScriptExtractor::new(), "src/w.ts"),
        (&TsxExtractor::new(), "src/w.tsx"),
    ];
    for (extractor, path) in extractors {
        let extraction = extractor
            .extract(Path::new(path), ECMASCRIPT_RECEIVER_SOURCE)
            .unwrap();
        let mut graph = ResolvedGraph::default();
        graph.files.insert(
            (*path).to_string(),
            FileEntry::from_extraction(ECMASCRIPT_RECEIVER_SOURCE, extraction),
        );
        resolve_graph(&mut graph);
        assert_resolved_receiver_edges(&graph.files[*path].edges, path);
    }
}

fn assert_resolved_receiver_edges(edges: &[SourceEdge], path: &str) {
    for &symbol in SHADOWED_RECEIVER_CALLS {
        let edge = edges
            .iter()
            .find(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == symbol)
            .unwrap();
        assert_shadowed_on(edge, "C");
    }
    for &symbol in LEXICAL_RECEIVER_CALLS {
        let edge = edges
            .iter()
            .find(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == symbol)
            .unwrap();
        assert_eq!(
            edge.provenance,
            EdgeProvenance::Receiver,
            "{path}: {edge:#?}"
        );
        assert!(
            edge.to.contains(&format!("C::{symbol}")),
            "{path}: {edge:#?}"
        );
    }
    let inner = edges
        .iter()
        .find(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == "inner")
        .unwrap();
    assert_eq!(
        inner.provenance,
        EdgeProvenance::Receiver,
        "{path}: {inner:#?}"
    );
    assert!(
        inner.to.contains("C::m::Inner::inner"),
        "{path}: {inner:#?}"
    );
}

#[cfg(feature = "source-graph-wave-b")]
#[test]
fn a_ruby_top_level_self_call_binds_the_top_level_method() {
    let extraction = crate::context::extract::ruby::RubyExtractor::new()
        .extract(
            Path::new("lib/main.rb"),
            b"def run\nend\n\ndef attach\n  self.run\nend\n",
        )
        .unwrap();

    let edge = call(&extraction, "run");
    assert_eq!(edge.from, "lib/main.rb#function:attach");
    assert_eq!(edge.to, "lib/main.rb#function:run", "edge: {edge:#?}");
    assert_eq!(
        edge.provenance,
        EdgeProvenance::LocalName,
        "edge: {edge:#?}"
    );
    assert_eq!(edge.receiver.as_deref(), Some("self"), "edge: {edge:#?}");
}
