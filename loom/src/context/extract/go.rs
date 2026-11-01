use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

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

; A method is qualified by its receiver's type, `T`, `*T`, `T[P]` or `*T[P]`,
; so `func (w *Widget) run()` scopes as `[Widget, run]` with no parent: Go
; cannot call it by its bare name.
(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: [
        (type_identifier) @definition.qualifier
        (pointer_type (type_identifier) @definition.qualifier)
        (generic_type type: (type_identifier) @definition.qualifier)
        (pointer_type (generic_type type: (type_identifier) @definition.qualifier))
      ]))
  name: (field_identifier) @name) @definition.function

; Every type spec or alias declares a type, a struct no more than a named
; slice, map, func or basic type: `type stack []int` owns the methods declared
; on it. Only an interface literal declares an interface.
(type_spec
  name: (type_identifier) @name
  type: [
    (array_type) (channel_type) (function_type) (generic_type) (map_type)
    (negated_type) (parenthesized_type) (pointer_type) (qualified_type)
    (slice_type) (struct_type) (type_identifier)
  ]) @definition.type

(type_alias
  name: (type_identifier) @name
  type: [
    (array_type) (channel_type) (function_type) (generic_type) (map_type)
    (negated_type) (parenthesized_type) (pointer_type) (qualified_type)
    (slice_type) (struct_type) (type_identifier)
  ]) @definition.type

(type_spec
  name: (type_identifier) @name
  type: (interface_type)) @definition.interface

(type_alias
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
  path: (interpreted_string_literal) @import.path) @import.statement

(call_expression
  function: (identifier) @call.name)

(call_expression
  function: (selector_expression
    operand: (_) @call.receiver
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
            dialect: "go",
            grammar_version: "0.25.0",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 4,
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

    /// One `import_spec`: `"a/b"` binds under its last segment, `x "a/b"` under
    /// `x`, `. "a/b"` is a glob, and `_ "a/b"` binds nothing.
    fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        let written_name = statement
            .split_once(['"', '`'])
            .map_or("", |(before, _)| before.trim());
        let (alias, glob) = match written_name {
            "" => (None, false),
            "." => (None, true),
            "_" => (Some(String::new()), false),
            name => (Some(name.to_string()), false),
        };
        vec![ImportBinding {
            path: path.to_string(),
            name: None,
            alias,
            glob,
            exported_as: None,
            site,
        }]
    }
}

impl SourceGraphExtractor for GoExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("go").expect("the dialect table names every registered extractor")
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
#[path = "go/tests_imports.rs"]
mod tests_imports;

#[cfg(test)]
#[path = "go/tests_methods.rs"]
mod tests_methods;

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
                "src/fixture.go#function:Use",
                "src/fixture.go#function:Widget::First",
                "src/fixture.go#function:Widget::Second",
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
        ("src/fixture.go", "<unresolved>", "imports", "syntax"),
        ("src/fixture.go", "<unresolved>", "imports", "syntax"),
        (
            "src/fixture.go",
            "src/fixture.go#constant:Count",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#constant:Value",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#function:Use",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#function:Widget::First",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#function:Widget::Second",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#interface:Runner",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#module:fixture",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go",
            "src/fixture.go#type:Widget",
            "contains",
            "structural",
        ),
        (
            "src/fixture.go#function:Use",
            "<unresolved>",
            "calls",
            "syntax",
        ),
        (
            "src/fixture.go#function:Widget::Second",
            "<unresolved>",
            "calls",
            "syntax",
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

        assert_eq!(edge.provenance, EdgeProvenance::Syntax);
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
