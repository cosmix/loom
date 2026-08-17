use std::path::Path;

use anyhow::Result;

use crate::context::extract::{
    run_query, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{NodeLanguage, SourceNodeKind};
use crate::language::DetectedLanguage;

/// Extracts Rust declarations, `use` paths, and direct or method calls.
///
/// It deliberately does not model closures, macros, cross-file resolution, or
/// the semantic relationships implied by traits and implementations; the
/// shared query harness records only syntactic containment and local calls.
pub struct RustExtractor;

impl RustExtractor {
    pub fn new() -> Self {
        RustExtractor
    }
}

impl Default for RustExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(function_item
  name: (identifier) @name) @definition.function

(struct_item
  name: (type_identifier) @name) @definition.type

(enum_item
  name: (type_identifier) @name) @definition.type

(type_item
  name: (type_identifier) @name) @definition.type

(trait_item
  name: (type_identifier) @name) @definition.interface

(mod_item
  name: (identifier) @name) @definition.module

(const_item
  name: (identifier) @name) @definition.constant

(static_item
  name: (identifier) @name) @definition.constant

(impl_item
  type: (type_identifier) @name) @definition.implementation

(use_declaration
  argument: (_) @import.path)

(call_expression
  function: (identifier) @call.name)

(call_expression
  function: (field_expression
    field: (field_identifier) @call.name))
"#;

impl QueryHarness for RustExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            grammar_version: "0.24.2",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 1,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::Rust
    }

    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind> {
        match suffix {
            "function" => Some(SourceNodeKind::Function),
            "type" => Some(SourceNodeKind::Type),
            "interface" => Some(SourceNodeKind::Interface),
            "module" => Some(SourceNodeKind::Module),
            "constant" => Some(SourceNodeKind::Constant),
            "implementation" => Some(SourceNodeKind::Implementation),
            _ => None,
        }
    }
}

impl SourceGraphExtractor for RustExtractor {
    fn language(&self) -> DetectedLanguage {
        DetectedLanguage::Rust
    }

    fn cache_identity(&self) -> ExtractorIdentity {
        QueryHarness::identity(self)
    }

    fn supports(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("rs")
        )
    }

    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<FileExtraction> {
        run_query(self, path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    const FIXTURE: &str = r#"
mod example {
    use crate::external::Thing;

    struct Widget;

    impl Widget {
        fn call_helper(&self) {
            self.helper();
        }

        fn helper(&self) {}
    }

    fn invoke_external() {
        missing_api();
    }

    trait Describable {}
}
"#;

    #[test]
    fn extracts_the_expected_node_ids() {
        let extraction = RustExtractor::new()
            .extract(Path::new("src/fixture.rs"), FIXTURE.as_bytes())
            .unwrap();
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
        let extraction = RustExtractor::new()
            .extract(Path::new("src/fixture.rs"), FIXTURE.as_bytes())
            .unwrap();

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

    /// Expected edges, kept at module scope so the assertion below stays
    /// short — the maintainability scanner budgets function bodies, not
    /// `const` declarations.
    const EXPECTED_EDGES: &[(&str, &str, &str, &str)] = &[
        ("src/fixture.rs", "<unresolved>", "imports", "inferred"),
        (
            "src/fixture.rs",
            "src/fixture.rs#module:example",
            "contains",
            "parser",
        ),
        (
            "src/fixture.rs#function:example::Widget::call_helper",
            "src/fixture.rs#function:example::Widget::helper",
            "calls",
            "parser",
        ),
        (
            "src/fixture.rs#function:example::invoke_external",
            "<unresolved>",
            "calls",
            "inferred",
        ),
        (
            "src/fixture.rs#implementation:example::Widget",
            "src/fixture.rs#function:example::Widget::call_helper",
            "contains",
            "parser",
        ),
        (
            "src/fixture.rs#implementation:example::Widget",
            "src/fixture.rs#function:example::Widget::helper",
            "contains",
            "parser",
        ),
        (
            "src/fixture.rs#module:example",
            "src/fixture.rs#function:example::invoke_external",
            "contains",
            "parser",
        ),
        (
            "src/fixture.rs#module:example",
            "src/fixture.rs#implementation:example::Widget",
            "contains",
            "parser",
        ),
        (
            "src/fixture.rs#module:example",
            "src/fixture.rs#interface:example::Describable",
            "contains",
            "parser",
        ),
        (
            "src/fixture.rs#module:example",
            "src/fixture.rs#type:example::Widget",
            "contains",
            "parser",
        ),
    ];

    #[test]
    fn extracts_the_expected_edges() {
        let extraction = RustExtractor::new()
            .extract(Path::new("src/fixture.rs"), FIXTURE.as_bytes())
            .unwrap();
        let mut edges: Vec<_> = extraction
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
            .collect();
        edges.sort_unstable();

        assert_eq!(edges, EXPECTED_EDGES);
    }

    #[test]
    fn marks_undefined_calls_as_low_confidence_inferred_edges() {
        let extraction = RustExtractor::new()
            .extract(Path::new("src/fixture.rs"), FIXTURE.as_bytes())
            .unwrap();
        let edge = extraction
            .edges
            .iter()
            .find(|edge| edge.symbol == "missing_api")
            .unwrap();

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
}
