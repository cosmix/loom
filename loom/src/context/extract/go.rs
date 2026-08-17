use std::path::Path;

use anyhow::Result;

use crate::context::extract::{
    run_query, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{NodeLanguage, SourceNodeKind};
use crate::language::DetectedLanguage;

/// Extracts top-level Go packages, named declarations, imports, and direct or
/// selected calls; it deliberately does not model local bindings, fields,
/// parameters, or cross-file resolution.
pub struct GoExtractor;

impl GoExtractor {
    pub fn new() -> Self {
        GoExtractor
    }
}

impl Default for GoExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @definition.function

(method_declaration
  name: (field_identifier) @name) @definition.function

(type_spec
  name: (type_identifier) @name
  type: (struct_type)) @definition.type

(type_spec
  name: (type_identifier) @name
  type: (interface_type)) @definition.interface

(package_clause
  (package_identifier) @name) @definition.module

(source_file
  (const_declaration
    (const_spec
      name: (identifier) @name) @definition.constant))

(source_file
  (var_declaration
    (var_spec
      name: (identifier) @name) @definition.constant))

; Both `import "x"` and a parenthesized group reach the path through
; `import_spec`; matching the spec directly covers the grouped form, whose
; specs hang off an `import_spec_list` rather than the declaration itself.
(import_spec
  path: (interpreted_string_literal) @import.path)

(call_expression
  function: (identifier) @call.name)

(call_expression
  function: (selector_expression
    field: (field_identifier) @call.name))
"#;

impl QueryHarness for GoExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_go::LANGUAGE.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            grammar_version: "0.25.0",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 1,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::Go
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

impl SourceGraphExtractor for GoExtractor {
    fn language(&self) -> DetectedLanguage {
        DetectedLanguage::Go
    }

    fn cache_identity(&self) -> ExtractorIdentity {
        QueryHarness::identity(self)
    }

    fn supports(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("go")
        )
    }

    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction> {
        run_query(self, path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::source_graph::EdgeProvenance;

    const FIXTURE: &str = r#"package fixture

import (
	"fmt"
	"strings"
)

const Value = "value"
var Count = 1

type Widget struct{}

type Runner interface {
	Run()
}

func (Widget) First() {}

func (Widget) Second() {
	Widget{}.First()
}

func Use() {
	fmt.Println(strings)
}
"#;

    #[test]
    fn extracts_the_expected_nodes() {
        let extraction = GoExtractor::new()
            .extract(Path::new("src/fixture.go"), FIXTURE.as_bytes())
            .unwrap();
        let mut ids = extraction
            .nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>();
        ids.sort_unstable();

        assert_eq!(
            ids,
            vec![
                "src/fixture.go",
                "src/fixture.go#constant:Count",
                "src/fixture.go#constant:Value",
                "src/fixture.go#function:First",
                "src/fixture.go#function:Second",
                "src/fixture.go#function:Use",
                "src/fixture.go#interface:Runner",
                "src/fixture.go#module:fixture",
                "src/fixture.go#type:Widget",
            ]
        );
    }

    /// Expected edges, kept at module scope so the assertion below stays
    /// short — the maintainability scanner budgets function bodies, not
    /// `const` declarations.
    const EXPECTED_EDGES: &[(&str, &str, &str, &str)] = &[
        ("src/fixture.go", "<unresolved>", "imports", "inferred"),
        ("src/fixture.go", "<unresolved>", "imports", "inferred"),
        (
            "src/fixture.go",
            "src/fixture.go#constant:Count",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#constant:Value",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#function:First",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#function:Second",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#function:Use",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#interface:Runner",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#module:fixture",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#type:Widget",
            "contains",
            "parser",
        ),
        (
            "src/fixture.go#function:Second",
            "src/fixture.go#function:First",
            "calls",
            "parser",
        ),
        (
            "src/fixture.go#function:Use",
            "<unresolved>",
            "calls",
            "inferred",
        ),
    ];

    #[test]
    fn extracts_the_expected_edges() {
        let extraction = GoExtractor::new()
            .extract(Path::new("src/fixture.go"), FIXTURE.as_bytes())
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
    fn undefined_calls_are_low_confidence_and_unresolved() {
        let extraction = GoExtractor::new()
            .extract(Path::new("src/fixture.go"), FIXTURE.as_bytes())
            .unwrap();
        let edge = extraction
            .edges
            .iter()
            .find(|edge| edge.symbol == "Println")
            .unwrap();

        assert_eq!(edge.provenance, EdgeProvenance::Inferred);
        assert!(edge.confidence <= 0.5);
        assert!(edge.is_unresolved());
    }

    #[test]
    fn syntax_errors_keep_only_the_file_node() {
        let extraction = GoExtractor::new()
            .extract(
                Path::new("src/broken.go"),
                b"package broken\nfunc broken( {\n",
            )
            .unwrap();

        assert_eq!(extraction.coverage.status(), "parse-error");
        assert_eq!(extraction.nodes.len(), 1);
    }
}
