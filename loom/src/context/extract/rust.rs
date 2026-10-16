use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

mod imports;

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

(mod_item
  name: (identifier) @import.path
  !body) @import.statement

(const_item
  name: (identifier) @name) @definition.constant

(static_item
  name: (identifier) @name) @definition.constant

; An `impl` is named by the base name of its self type, however that type is
; written: `impl Foo`, `impl<T> Foo<T>`, `impl<'a> Tr for Foo<'a>`,
; `impl crate::a::Foo` and `impl<T> a::Foo<T>` all scope their members as `Foo`.
(impl_item
  type: (type_identifier) @name) @definition.implementation

(impl_item
  type: (generic_type
    type: (type_identifier) @name)) @definition.implementation

(impl_item
  type: (scoped_type_identifier
    name: (type_identifier) @name)) @definition.implementation

(impl_item
  type: (generic_type
    type: (scoped_type_identifier
      name: (type_identifier) @name))) @definition.implementation

(use_declaration
  argument: (_) @import.path) @import.statement

(call_expression
  function: (identifier) @call.name)

(call_expression
  function: (field_expression
    value: (_) @call.receiver
    field: (field_identifier) @call.name))

; `Self::helper()` names a member of the enclosing type: its receiver is `Self`.
(call_expression
  function: (scoped_identifier
    path: (identifier) @call.receiver
    name: (identifier) @call.name)
  (#eq? @call.receiver "Self"))

; A qualified callee — `crate::a::b()`, `super::b()`, `Widget::new()` — is one
; `scoped_identifier`, captured whole so the qualifier survives into resolution.
; A `Self` path is the receiver form above.
(call_expression
  function: (scoped_identifier
    path: (_) @_path) @call.name
  (#not-eq? @_path "Self"))

; The same forms carrying a turbofish: `b::<T>()`, `Widget::new::<T>()`,
; `Self::new::<T>()` and `value.parse::<T>()`.
(call_expression
  function: (generic_function
    function: (identifier) @call.name))

(call_expression
  function: (generic_function
    function: (scoped_identifier
      path: (identifier) @call.receiver
      name: (identifier) @call.name))
  (#eq? @call.receiver "Self"))

(call_expression
  function: (generic_function
    function: (scoped_identifier
      path: (_) @_path) @call.name)
  (#not-eq? @_path "Self"))

(call_expression
  function: (generic_function
    function: (field_expression
      value: (_) @call.receiver
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
            dialect: "rust",
            grammar_version: "0.24.2",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 5,
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

    fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        if statement.split_whitespace().any(|word| word == "mod") {
            return vec![ImportBinding {
                path: path.into(),
                name: None,
                alias: None,
                glob: false,
                exported_as: None,
                site,
            }];
        }
        imports::use_bindings(path, site)
    }
}

impl SourceGraphExtractor for RustExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("rust").expect("the dialect table names every registered extractor")
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
#[path = "rust/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rust/tests_imports.rs"]
mod tests_imports;

#[cfg(test)]
#[path = "rust/tests_impls.rs"]
mod tests_impls;
