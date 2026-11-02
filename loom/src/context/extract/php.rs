use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

mod imports;

/// Extracts declarations from `.php` files, including `<?php` blocks inside HTML. A method without a body (abstract, interface) is not a definition.
/// A trait `use` inside a class is a `References` edge from the class to the trait.
pub struct PhpExtractor;

impl PhpExtractor {
    pub fn new() -> Self {
        PhpExtractor
    }
}

impl Default for PhpExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(namespace_definition
  name: (namespace_name) @name) @definition.module

(class_declaration
  name: (name) @name) @definition.type

(trait_declaration
  name: (name) @name) @definition.type

(enum_declaration
  name: (name) @name) @definition.type

(interface_declaration
  name: (name) @name) @definition.interface

(function_definition
  name: (name) @name) @definition.function

(method_declaration
  name: (name) @name
  body: (_)) @definition.function

; `use A\B;`, `use A\B as C;`, `use function A\f;` and each member of a group
; `use A\{B, C as D};`. The first named child is the path: an alias is a second
; `name`. `import_spec` completes a group member's path from the statement.
(namespace_use_declaration
  (namespace_use_clause
    .
    [
      (name)
      (qualified_name)
    ] @import.path)) @import.statement

(namespace_use_declaration
  body: (namespace_use_group
    (namespace_use_clause
      .
      [
        (name)
        (qualified_name)
      ] @import.path))) @import.statement

; `require`, `require_once`, `include` and `include_once` of a plain string.
; A computed path (`__DIR__ . '/x.php'`) or a parenthesized one is no import.
(require_expression
  [
    (string
      .
      (string_content) @import.path
      .)
    (encapsed_string
      .
      (string_content) @import.path
      .)
  ]) @import.statement

(require_once_expression
  [
    (string
      .
      (string_content) @import.path
      .)
    (encapsed_string
      .
      (string_content) @import.path
      .)
  ]) @import.statement

(include_expression
  [
    (string
      .
      (string_content) @import.path
      .)
    (encapsed_string
      .
      (string_content) @import.path
      .)
  ]) @import.statement

(include_once_expression
  [
    (string
      .
      (string_content) @import.path
      .)
    (encapsed_string
      .
      (string_content) @import.path
      .)
  ]) @import.statement

; `use Loggable;` in a class body: a `References` edge from the class to the
; trait, whose methods are the class's own.
(use_declaration
  [
    (name)
    (qualified_name)
  ] @reference.name)

; A call through a variable (`$f()`) or a qualified name (`A\f()`) has no
; `name` callee and is not captured.
(function_call_expression
  function: (name) @call.name)

(member_call_expression
  object: (_) @call.receiver
  name: (name) @call.name)

(nullsafe_member_call_expression
  object: (_) @call.receiver
  name: (name) @call.name)

; `self::m()`, `static::m()` and `A::m()`: the callee is `m` and the scope is
; the receiver; no `A::m` symbol is made.
(scoped_call_expression
  scope: (_) @call.receiver
  name: (name) @call.name)
"#;

impl QueryHarness for PhpExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_php::LANGUAGE_PHP.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "php",
            grammar_version: "0.24.2",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 4,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::Php
    }

    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind> {
        match suffix {
            "function" => Some(SourceNodeKind::Function),
            "type" => Some(SourceNodeKind::Type),
            "interface" => Some(SourceNodeKind::Interface),
            "module" => Some(SourceNodeKind::Module),
            _ => None,
        }
    }

    /// A namespace name is one scope segment: `A\B` is `A.B`.
    fn definition_name(&self, name: &str) -> String {
        name.replace('\\', ".")
    }

    /// A `use` group member captures only its tail; the spec is the full path.
    /// A `require` or `include` path is the spec as written.
    fn import_spec(&self, statement: &str, path: &str) -> String {
        if !imports::is_use(statement) {
            return path.to_string();
        }
        let clauses = imports::clauses(statement);
        imports::find(&clauses, path).map_or_else(|| path.to_string(), |clause| clause.path.clone())
    }

    /// A `use` clause binds its path under its last segment or its `as` alias.
    /// A `require` or `include` makes everything the file defines visible: one
    /// glob binding.
    fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        let is_use = imports::is_use(statement);
        let alias = if is_use {
            let clauses = imports::clauses(statement);
            imports::find(&clauses, path).and_then(|clause| clause.alias.clone())
        } else {
            None
        };
        vec![ImportBinding {
            path: path.to_string(),
            name: None,
            alias,
            glob: !is_use,
            exported_as: None,
            site,
        }]
    }
}

impl SourceGraphExtractor for PhpExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("php").expect("the dialect table names every registered extractor")
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            declarations: true,
            imports: true,
            import_bindings: true,
            calls: true,
            receivers: true,
            references: true,
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
mod tests;
