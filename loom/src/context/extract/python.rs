use std::path::Path;

use anyhow::Result;

use crate::context::extract::{
    run_query, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{NodeLanguage, SourceNodeKind};
use crate::language::DetectedLanguage;

/// Extracts named Python functions and classes, imports, and direct or attribute calls;
/// module-level assignments are deliberately excluded because their targets are ambiguous
/// in a simple grammar query.
pub struct PythonExtractor;

impl PythonExtractor {
    pub fn new() -> Self {
        PythonExtractor
    }
}

impl Default for PythonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(function_definition
  name: (identifier) @name) @definition.function

(class_definition
  name: (identifier) @name) @definition.type

(import_statement
  name: (dotted_name) @import.path)

(import_from_statement
  module_name: (dotted_name) @import.path)

(call
  function: (identifier) @call.name)

(call
  function: (attribute
    attribute: (identifier) @call.name))
"#;

impl QueryHarness for PythonExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_python::LANGUAGE.into()
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
        NodeLanguage::Python
    }

    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind> {
        match suffix {
            "function" => Some(SourceNodeKind::Function),
            "type" => Some(SourceNodeKind::Type),
            _ => None,
        }
    }
}

impl SourceGraphExtractor for PythonExtractor {
    fn language(&self) -> DetectedLanguage {
        DetectedLanguage::Python
    }

    fn cache_identity(&self) -> ExtractorIdentity {
        QueryHarness::identity(self)
    }

    fn supports(&self, path: &Path) -> bool {
        matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("py") | Some("pyi")
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
    use crate::context::extract::SourceGraphExtractor;
    use crate::context::source_graph::EdgeProvenance;

    const FIXTURE: &str = r#"import x
from y import z

class Widget:
    def first(self):
        self.second()

    def second(self):
        def inner():
            unknown()
        inner()

def run():
    z()
"#;

    #[test]
    fn extracts_the_expected_nodes() {
        let extraction = PythonExtractor::new()
            .extract(Path::new("src/fixture.py"), FIXTURE.as_bytes())
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
                "src/fixture.py",
                "src/fixture.py#function:Widget::first",
                "src/fixture.py#function:Widget::second",
                "src/fixture.py#function:Widget::second::inner",
                "src/fixture.py#function:run",
                "src/fixture.py#type:Widget",
            ]
        );
    }

    /// Expected edges, kept at module scope so the assertion below stays
    /// short — the maintainability scanner budgets function bodies, not
    /// `const` declarations.
    const EXPECTED_EDGES: &[(&str, &str, &str, &str)] = &[
        ("src/fixture.py", "<unresolved>", "imports", "inferred"),
        ("src/fixture.py", "<unresolved>", "imports", "inferred"),
        (
            "src/fixture.py",
            "src/fixture.py#function:run",
            "contains",
            "parser",
        ),
        (
            "src/fixture.py",
            "src/fixture.py#type:Widget",
            "contains",
            "parser",
        ),
        (
            "src/fixture.py#function:Widget::first",
            "src/fixture.py#function:Widget::second",
            "calls",
            "parser",
        ),
        (
            "src/fixture.py#function:Widget::second",
            "src/fixture.py#function:Widget::second::inner",
            "calls",
            "parser",
        ),
        (
            "src/fixture.py#function:Widget::second",
            "src/fixture.py#function:Widget::second::inner",
            "contains",
            "parser",
        ),
        (
            "src/fixture.py#function:Widget::second::inner",
            "<unresolved>",
            "calls",
            "inferred",
        ),
        (
            "src/fixture.py#function:run",
            "<unresolved>",
            "calls",
            "inferred",
        ),
        (
            "src/fixture.py#type:Widget",
            "src/fixture.py#function:Widget::first",
            "contains",
            "parser",
        ),
        (
            "src/fixture.py#type:Widget",
            "src/fixture.py#function:Widget::second",
            "contains",
            "parser",
        ),
    ];

    #[test]
    fn extracts_the_expected_edges() {
        let extraction = PythonExtractor::new()
            .extract(Path::new("src/fixture.py"), FIXTURE.as_bytes())
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
    fn marks_unknown_calls_as_unresolved_inferences() {
        let extraction = PythonExtractor::new()
            .extract(Path::new("src/fixture.py"), FIXTURE.as_bytes())
            .unwrap();
        let edge = extraction
            .edges
            .iter()
            .find(|edge| edge.symbol == "unknown")
            .unwrap();

        assert_eq!(edge.provenance, EdgeProvenance::Inferred);
        assert!(edge.confidence <= 0.5);
        assert!(edge.is_unresolved());
    }

    #[test]
    fn syntax_errors_keep_only_the_file_node() {
        let extraction = PythonExtractor::new()
            .extract(Path::new("src/broken.py"), b"def broken(:\n")
            .unwrap();

        assert_eq!(extraction.coverage.status(), "parse-error");
        assert_eq!(extraction.nodes.len(), 1);
    }
}
