//! What a Rust `use` declaration binds, and what an imported name may claim.

use std::path::Path;

use super::*;
use crate::context::source_graph::{EdgeProvenance, Span};

/// One binding as `(path, name, alias, glob)`.
type Leaf = (String, Option<String>, Option<String>, bool);

/// The bindings of `source`, in the order the extraction reports them.
fn bindings(source: &str) -> Vec<Leaf> {
    RustExtractor::new()
        .extract(Path::new("src/lib.rs"), source.as_bytes())
        .unwrap()
        .imports
        .into_iter()
        .map(|binding| (binding.path, binding.name, binding.alias, binding.glob))
        .collect()
}

fn leaf(path: &str, name: Option<&str>, alias: Option<&str>, glob: bool) -> Leaf {
    (
        path.to_string(),
        name.map(str::to_string),
        alias.map(str::to_string),
        glob,
    )
}

#[test]
fn a_plain_use_binds_its_last_segment() {
    assert_eq!(
        bindings("use a::b::c;\n"),
        vec![leaf("a::b::c", Some("c"), None, false)]
    );
}

#[test]
fn an_alias_is_kept() {
    assert_eq!(
        bindings("use a::b::c as d;\n"),
        vec![leaf("a::b::c", Some("c"), Some("d"), false)]
    );
}

#[test]
fn an_underscore_alias_binds_no_name() {
    let bindings = imports::use_bindings("a::Tr as _", Span::default());

    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].alias.as_deref(), Some(""));
    assert_eq!(bindings[0].local_name(), None);
}

#[test]
fn a_glob_binds_the_module_it_globs() {
    assert_eq!(
        bindings("use a::b::*;\n"),
        vec![leaf("a::b", None, None, true)]
    );
}

#[test]
fn a_brace_group_expands_to_one_binding_per_leaf() {
    assert_eq!(
        bindings("use a::{self, b, c as d, e::*, f::{g, h as i}};\n"),
        vec![
            leaf("a", None, None, false),
            leaf("a::b", Some("b"), None, false),
            leaf("a::c", Some("c"), Some("d"), false),
            leaf("a::e", None, None, true),
            leaf("a::f::g", Some("g"), None, false),
            leaf("a::f::h", Some("h"), Some("i"), false),
        ]
    );
}

#[test]
fn a_group_wrapped_over_several_lines_reads_like_one_line() {
    assert_eq!(
        bindings("use crate::x::{\n    one,\n    two as\n        three,\n};\n"),
        vec![
            leaf("crate::x::one", Some("one"), None, false),
            leaf("crate::x::two", Some("two"), Some("three"), false),
        ]
    );
}

#[test]
fn comments_in_use_trees_preserve_bindings() {
    assert_eq!(
        bindings("use external::{parse, nested::{item as alias, self}};"),
        bindings("use external::{/* outer /* nested */ comment */ parse, // comment\n nested::{item /*x*/ as /*y*/ alias, self}};")
    );
}

#[test]
fn commented_external_use_refuses_cross_file_namesake() {
    use crate::context::graph_store::{FileEntry, ResolvedGraph};
    use crate::context::resolve_graph;
    use crate::context::source_graph::SourceEdgeKind;
    for import in ["use external::parse;", "use external::{/* why */ parse};"] {
        let source = format!("{import} fn run() {{ parse(); }}");
        let extractor = RustExtractor::new();
        let entry = |path, bytes: &[u8]| {
            let value = extractor.extract(Path::new(path), bytes).unwrap();
            FileEntry {
                nodes: value.nodes,
                edges: value.edges,
                coverage: value.coverage,
                imports: value.imports,
                ..FileEntry::default()
            }
        };
        let mut graph = ResolvedGraph::default();
        graph
            .files
            .insert("src/lib.rs".into(), entry("src/lib.rs", source.as_bytes()));
        graph.files.insert(
            "src/other.rs".into(),
            entry("src/other.rs", b"pub fn parse() {}"),
        );
        resolve_graph(&mut graph);
        let call = graph.files["src/lib.rs"]
            .edges
            .iter()
            .find(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == "parse")
            .unwrap();
        assert!(call.is_unresolved(), "{call:#?}");
    }
}

/// `use external_crate::helper;` binds `helper` locally, so a call to `helper`
/// is not claimed by a same-named definition in a module the caller cannot see.
#[test]
fn an_imported_name_is_not_bound_locally() {
    let extraction = RustExtractor::new()
        .extract(
            Path::new("src/lib.rs"),
            b"mod hidden {\n    pub fn helper() {}\n}\nuse external_crate::helper;\nfn run() {\n    helper();\n}\n",
        )
        .unwrap();
    let binding = &extraction.imports[0];
    let edge = extraction
        .edges
        .iter()
        .find(|edge| edge.symbol == "helper")
        .unwrap();

    assert!(!binding.glob);
    assert_eq!(binding.local_name(), Some("helper"));
    assert_eq!(edge.provenance, EdgeProvenance::Syntax);
    assert!(edge.is_unresolved());
}

/// `use a::{{...{b}...}};` with `depth` groups.
fn nested_use(depth: usize) -> String {
    format!("use a::{}b{};\n", "{".repeat(depth), "}".repeat(depth))
}

#[test]
fn a_nested_group_within_the_cap_expands_fully() {
    assert_eq!(
        bindings("use a::{{b, {c, d}}};\n"),
        vec![
            leaf("a::b", Some("b"), None, false),
            leaf("a::c", Some("c"), None, false),
            leaf("a::d", Some("d"), None, false),
        ]
    );
    assert_eq!(
        bindings(&nested_use(imports::MAX_USE_TREE_DEPTH)),
        vec![leaf("a::b", Some("b"), None, false)]
    );
}

#[test]
fn groups_past_the_cap_bind_nothing() {
    assert!(bindings(&nested_use(imports::MAX_USE_TREE_DEPTH + 1)).is_empty());
}

#[test]
fn a_pathologically_deep_group_is_bounded() {
    let found = bindings(&nested_use(10_000));

    assert!(found.len() <= 1);
}
