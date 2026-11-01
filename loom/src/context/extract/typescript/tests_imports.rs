//! What TypeScript imports and re-exports bind, and what a `this` call claims.

use std::path::Path;

use super::*;
use crate::context::source_graph::{EdgeProvenance, SourceEdgeKind, RECEIVER_CONFIDENCE};

/// One binding as `(path, name, alias, glob)`.
type Leaf = (String, Option<String>, Option<String>, bool);

fn extraction(source: &str) -> FileExtraction {
    TypeScriptExtractor::new()
        .extract(Path::new("src/lib.ts"), source.as_bytes())
        .unwrap()
}

/// The bindings of `source`, in the order the extraction reports them: by
/// site, then path and name.
fn bindings(source: &str) -> Vec<Leaf> {
    extraction(source)
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
fn named_imports_keep_their_aliases() {
    assert_eq!(
        bindings("import { a, b as c, type D } from \"x\";\n"),
        vec![
            leaf("x", Some("D"), Some("D"), false),
            leaf("x", Some("a"), Some("a"), false),
            leaf("x", Some("b"), Some("c"), false),
        ]
    );
}

#[test]
fn comments_in_imports_preserve_bindings() {
    let clean =
        "import d, { parse, type Thing as T } from './x'; export { parse as p } from 'external';";
    let commented = "import d, { /*x*/ parse, type /*y*/ Thing as /*z*/ T } from './x'; export { /*a*/ parse as p } from 'external';";
    assert_eq!(bindings(clean), bindings(commented));
}

#[test]
fn comments_preserve_bindings_in_every_ecmascript_grammar() {
    use crate::context::extract::{javascript::JavaScriptExtractor, tsx::TsxExtractor};
    let extractors: &[(&dyn SourceGraphExtractor, &str)] = &[
        (&JavaScriptExtractor::new(), "a.js"),
        (&TypeScriptExtractor::new(), "a.ts"),
        (&TsxExtractor::new(), "a.tsx"),
    ];
    for (extractor, path) in extractors {
        let project = |source: &str| {
            extractor
                .extract(Path::new(path), source.as_bytes())
                .unwrap()
                .imports
                .into_iter()
                .map(|b| (b.path, b.name, b.alias, b.glob, b.exported_as))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            project("import { parse as p } from 'external'; export { p as q } from './local';"),
            project("import { /* x */ parse as /* y */ p } from 'external'; export { /* z */ p as q } from './local';"),
            "{path}"
        );
    }
}

#[test]
fn comment_markers_in_module_strings_remain_literal() {
    assert_eq!(
        bindings("import { /* comment */ parse } from './x/*literal*/y';"),
        vec![leaf("./x/*literal*/y", Some("parse"), Some("parse"), false)]
    );
}

#[test]
fn commented_external_import_refuses_cross_file_namesake() {
    use crate::context::graph_store::{FileEntry, ResolvedGraph};
    use crate::context::resolve_graph;
    for import in [
        "import { parse } from 'external';",
        "import { /* explanation */ parse } from 'external';",
    ] {
        let source = format!("{import} function run() {{ parse(); }}");
        let main = extraction(&source);
        let other = TypeScriptExtractor::new()
            .extract(Path::new("src/other.ts"), b"export function parse() {} ")
            .unwrap();
        let entry = |value: FileExtraction| FileEntry {
            nodes: value.nodes,
            edges: value.edges,
            coverage: value.coverage,
            imports: value.imports,
            ..FileEntry::default()
        };
        let mut graph = ResolvedGraph::default();
        graph.files.insert("src/lib.ts".into(), entry(main));
        graph.files.insert("src/other.ts".into(), entry(other));
        resolve_graph(&mut graph);
        let call = graph.files["src/lib.ts"]
            .edges
            .iter()
            .find(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == "parse")
            .unwrap();
        assert!(call.is_unresolved(), "{call:#?}");
    }
}

#[test]
fn a_default_import_names_the_default_export() {
    assert_eq!(
        bindings("import d from \"x\";\nimport e, { f } from 'y';\n"),
        vec![
            leaf("x", Some("default"), Some("d"), false),
            leaf("y", Some("default"), Some("e"), false),
            leaf("y", Some("f"), Some("f"), false),
        ]
    );
}

#[test]
fn a_namespace_import_binds_the_whole_module() {
    let found = extraction("import * as ns from \"x\";\n").imports;

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, None);
    assert_eq!(found[0].local_name(), Some("ns"));
}

#[test]
fn a_side_effect_import_binds_nothing() {
    let found = extraction("import \"./setup\";\n").imports;

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, "./setup");
    assert_eq!(found[0].alias.as_deref(), Some(""));
    assert_eq!(found[0].local_name(), None);
}

#[test]
fn a_re_export_binds_no_local_name_and_a_star_re_export_is_a_glob() {
    assert_eq!(
        bindings("export { a as b } from \"x\";\nexport * from \"y\";\n"),
        vec![
            leaf("x", Some("a"), Some(""), false),
            leaf("y", None, None, true),
        ]
    );
}

#[test]
fn a_re_export_records_the_name_it_exports() {
    let exported: Vec<Option<String>> =
        extraction("export { a as b, c } from \"x\";\nimport { d } from \"y\";\n")
            .imports
            .into_iter()
            .map(|binding| binding.exported_as)
            .collect();
    assert_eq!(
        exported,
        [Some("b".to_string()), Some("c".to_string()), None]
    );

    let namespace = imports::statement_bindings("export * as ns from \"z\";", "z", Span::default());
    assert_eq!(namespace.len(), 1);
    assert_eq!(namespace[0].exported_as.as_deref(), Some("ns"));
    assert_eq!(namespace[0].local_name(), None);
}

#[test]
fn a_this_call_binds_to_the_enclosing_class_by_receiver() {
    let extraction = extraction("class S {\n  a() {\n    this.b();\n  }\n  b() {}\n}\n");
    let edge = extraction
        .edges
        .iter()
        .find(|edge| edge.symbol == "b")
        .unwrap();

    assert_eq!(edge.to, "src/lib.ts#function:S::b");
    assert_eq!(edge.provenance, EdgeProvenance::Receiver);
    assert_eq!(edge.confidence, RECEIVER_CONFIDENCE);
    assert_eq!(edge.receiver.as_deref(), Some("this"));
}
