use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

mod imports;

/// Extracts definitions, imports, and calls from `.ts`, `.mts`, and `.cts`
/// files. It excludes `.tsx`, which needs the separately pinned `LANGUAGE_TSX`
/// grammar and belongs to the `tsx` dialect's own extractor.
pub struct TypeScriptExtractor;

impl TypeScriptExtractor {
    pub fn new() -> Self {
        TypeScriptExtractor
    }
}

impl Default for TypeScriptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @definition.function

(method_definition
  name: (property_identifier) @name) @definition.function

(variable_declarator
  name: (identifier) @name
  value: [(arrow_function) (function_expression) (generator_function)]) @definition.function

(class_declaration
  name: (type_identifier) @name) @definition.type

(abstract_class_declaration
  name: (type_identifier) @name) @definition.type

(interface_declaration
  name: (type_identifier) @name) @definition.interface

(type_alias_declaration
  name: (type_identifier) @name) @definition.type

(enum_declaration
  name: (identifier) @name) @definition.type

(module
  name: (identifier) @name) @definition.module

(
  (export_statement
    declaration: (lexical_declaration
      "const"
      (variable_declarator
        name: (identifier) @name
        value: (_) @_value) @definition.constant))
  (#not-match? @definition.constant "=>")
  (#not-match? @_value "^(async\\b\\s*)?function\\b")
)

(import_statement
  source: (string) @import.path) @import.statement

(export_statement
  source: (string) @import.path) @import.statement

(call_expression
  function: (identifier) @call.name)

(call_expression
  function: (member_expression
    object: (_) @call.receiver
    property: (property_identifier) @call.name))
"#;

/// The bindings of one `import` or `export ... from` statement, shared by every
/// ECMAScript dialect.
pub(super) fn ecmascript_import_bindings(
    statement: &str,
    path: &str,
    site: Span,
) -> Vec<ImportBinding> {
    imports::statement_bindings(statement, path, site)
}

impl QueryHarness for TypeScriptExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "typescript",
            grammar_version: "0.23.2",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 6,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::TypeScript
    }

    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind> {
        match suffix {
            "function" => Some(SourceNodeKind::Function),
            "type" => Some(SourceNodeKind::Type),
            "interface" => Some(SourceNodeKind::Interface),
            "module" => Some(SourceNodeKind::Module),
            "constant" => Some(SourceNodeKind::Constant),
            _ => None,
        }
    }

    fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        ecmascript_import_bindings(statement, path, site)
    }
}

impl SourceGraphExtractor for TypeScriptExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("typescript").expect("the dialect table names every registered extractor")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            declarations: true,
            imports: true,
            import_bindings: true,
            calls: true,
            receivers: true,
            references: false,
        }
    }

    fn cache_identity(&self) -> ExtractorIdentity {
        QueryHarness::identity(self)
    }

    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction> {
        run_query(self, path, bytes)
    }
}

#[cfg(test)]
#[path = "typescript/tests_imports.rs"]
mod tests_imports;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::context::source_graph::{EdgeProvenance, SourceEdgeKind};

    use super::*;

    const FIXTURE: &str = r#"
import { remote as importedName } from "./remote";
export { published as reexported } from "./public";

interface Greeter {
    greet(): void;
}

class Service {
    first() {
        this.second();
    }

    second() {}
}

export const VERSION = "1";
export const run = () => importedName();
"#;

    #[test]
    fn extracts_expected_node_ids() {
        let extraction = TypeScriptExtractor::new()
            .extract(Path::new("src/fixture.ts"), FIXTURE.as_bytes())
            .unwrap();
        let mut node_ids = extraction
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
        node_ids.sort_unstable();

        assert_eq!(
            node_ids,
            vec![
                "src/fixture.ts",
                "src/fixture.ts#constant:VERSION",
                "src/fixture.ts#function:Service::first",
                "src/fixture.ts#function:Service::second",
                "src/fixture.ts#function:run",
                "src/fixture.ts#interface:Greeter",
                "src/fixture.ts#type:Service",
            ]
        );
    }

    /// Expected edges, kept at module scope so the assertion below stays
    /// short — the maintainability scanner budgets function bodies, not
    /// `const` declarations.
    const EXPECTED_EDGES: &[(&str, &str, &str, &str)] = &[
        ("src/fixture.ts", "<unresolved>", "imports", "syntax"),
        ("src/fixture.ts", "<unresolved>", "imports", "syntax"),
        (
            "src/fixture.ts",
            "src/fixture.ts#constant:VERSION",
            "contains",
            "structural",
        ),
        (
            "src/fixture.ts",
            "src/fixture.ts#function:run",
            "contains",
            "structural",
        ),
        (
            "src/fixture.ts",
            "src/fixture.ts#interface:Greeter",
            "contains",
            "structural",
        ),
        (
            "src/fixture.ts",
            "src/fixture.ts#type:Service",
            "contains",
            "structural",
        ),
        (
            "src/fixture.ts#function:Service::first",
            "src/fixture.ts#function:Service::second",
            "calls",
            "receiver",
        ),
        (
            "src/fixture.ts#function:run",
            "<unresolved>",
            "calls",
            "syntax",
        ),
        (
            "src/fixture.ts#type:Service",
            "src/fixture.ts#function:Service::first",
            "contains",
            "structural",
        ),
        (
            "src/fixture.ts#type:Service",
            "src/fixture.ts#function:Service::second",
            "contains",
            "structural",
        ),
    ];

    #[test]
    fn extracts_expected_edges() {
        let extraction = TypeScriptExtractor::new()
            .extract(Path::new("src/fixture.ts"), FIXTURE.as_bytes())
            .unwrap();
        let mut edges = extraction
            .edges
            .iter()
            .map(|edge| {
                (
                    edge.from.as_str(),
                    edge.to.as_str(),
                    edge.kind.as_str(),
                    edge.provenance.as_str(),
                )
            })
            .collect::<Vec<_>>();
        edges.sort_unstable();

        assert_eq!(edges, EXPECTED_EDGES);
    }

    #[test]
    fn imported_calls_are_unresolved_inferred_edges() {
        let extraction = TypeScriptExtractor::new()
            .extract(Path::new("src/fixture.ts"), FIXTURE.as_bytes())
            .unwrap();
        let edge = extraction
            .edges
            .iter()
            .find(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == "importedName")
            .unwrap();

        assert_eq!(edge.provenance, EdgeProvenance::Syntax);
        assert!(edge.confidence <= 0.5);
        assert!(edge.is_unresolved());
    }

    #[test]
    fn syntax_errors_keep_only_the_file_node() {
        let extraction = TypeScriptExtractor::new()
            .extract(Path::new("src/fixture.ts"), b"export const broken = ;\n")
            .unwrap();

        assert_eq!(extraction.coverage.status(), "parse-error");
        assert_eq!(extraction.nodes.len(), 1);
    }
}
