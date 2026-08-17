use std::path::Path;

use anyhow::Result;

use crate::context::extract::{
    run_query, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{NodeLanguage, SourceNodeKind};
use crate::language::DetectedLanguage;

/// Extracts definitions, imports, and calls from `.ts`, `.mts`, and `.cts`
/// files. It deliberately excludes `.tsx`: that needs the separately pinned
/// `LANGUAGE_TSX` grammar, so `.tsx` files fall through to the file-level lexical node.
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
  value: (arrow_function)) @definition.function

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
        name: (identifier) @name) @definition.constant))
  (#not-match? @definition.constant "=>")
)

(import_statement
  source: (string) @import.path)

(export_statement
  source: (string) @import.path)

(call_expression
  function: (identifier) @call.name)

(call_expression
  function: (member_expression
    property: (property_identifier) @call.name))
"#;

impl QueryHarness for TypeScriptExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            grammar_version: "0.23.2",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 1,
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
}

impl SourceGraphExtractor for TypeScriptExtractor {
    fn language(&self) -> DetectedLanguage {
        DetectedLanguage::TypeScript
    }

    fn cache_identity(&self) -> ExtractorIdentity {
        QueryHarness::identity(self)
    }

    fn supports(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("ts") | Some("mts") | Some("cts")
        )
    }

    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction> {
        run_query(self, path, bytes)
    }
}

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
        ("src/fixture.ts", "<unresolved>", "imports", "inferred"),
        ("src/fixture.ts", "<unresolved>", "imports", "inferred"),
        (
            "src/fixture.ts",
            "src/fixture.ts#constant:VERSION",
            "contains",
            "parser",
        ),
        (
            "src/fixture.ts",
            "src/fixture.ts#function:run",
            "contains",
            "parser",
        ),
        (
            "src/fixture.ts",
            "src/fixture.ts#interface:Greeter",
            "contains",
            "parser",
        ),
        (
            "src/fixture.ts",
            "src/fixture.ts#type:Service",
            "contains",
            "parser",
        ),
        (
            "src/fixture.ts#function:Service::first",
            "src/fixture.ts#function:Service::second",
            "calls",
            "parser",
        ),
        (
            "src/fixture.ts#function:run",
            "<unresolved>",
            "calls",
            "inferred",
        ),
        (
            "src/fixture.ts#type:Service",
            "src/fixture.ts#function:Service::first",
            "contains",
            "parser",
        ),
        (
            "src/fixture.ts#type:Service",
            "src/fixture.ts#function:Service::second",
            "contains",
            "parser",
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

        assert_eq!(edge.provenance, EdgeProvenance::Inferred);
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
