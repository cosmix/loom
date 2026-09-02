use std::path::Path;

use anyhow::Result;

use crate::context::extract::{
    run_query, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{NodeLanguage, SourceNodeKind};
use crate::language::DetectedLanguage;

/// Extracts Rust declarations, `use` paths, and calls.
///
/// A call is captured however it is written: a bare `helper()`, a method
/// `self.helper()`, and a qualified `crate::a::b()`, `super::b()` or
/// `Widget::new()` all produce a call reference. A qualified callee keeps the
/// path as written, minus any turbofish, so resolution can try the qualified
/// spelling before it falls back to the bare name.
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

; A qualified callee — `crate::a::b()`, `super::b()`, `Widget::new()` — is one
; `scoped_identifier`, captured whole so the qualifier survives into resolution.
(call_expression
  function: (scoped_identifier) @call.name)

; The same three forms carrying a turbofish: `b::<T>()`, `Widget::new::<T>()`,
; and `value.parse::<T>()`.
(call_expression
  function: (generic_function
    function: [
      (identifier)
      (scoped_identifier)
    ] @call.name))

(call_expression
  function: (generic_function
    function: (field_expression
      field: (field_identifier) @call.name)))
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
            extractor_version: 2,
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
#[path = "rust/tests.rs"]
mod tests;
