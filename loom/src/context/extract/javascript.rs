use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::ecmascript::jsx_patterns;
use crate::context::extract::typescript::ecmascript_import_bindings;
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

mod require;

/// Extracts definitions, imports (ES modules and CommonJS `require`), calls and
/// JSX element uses from `.js`, `.mjs`, `.cjs` and `.jsx` files.
pub struct JavaScriptExtractor;

impl JavaScriptExtractor {
    pub fn new() -> Self {
        JavaScriptExtractor
    }
}

impl Default for JavaScriptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Declarations, imports, re-exports, CommonJS `require` and calls, followed by
/// the JSX patterns shared with the TSX extractor.
const QUERY: &str = concat!(
    r#"
(function_declaration
  name: (identifier) @name) @definition.function

(generator_function_declaration
  name: (identifier) @name) @definition.function

(method_definition
  name: (property_identifier) @name) @definition.function

[
  (lexical_declaration
    (variable_declarator
      name: (identifier) @name
      value: [(arrow_function) (function_expression) (generator_function)]))
  (variable_declaration
    (variable_declarator
      name: (identifier) @name
      value: [(arrow_function) (function_expression) (generator_function)]))
] @definition.function

(class_declaration
  name: (identifier) @name) @definition.type

(export_statement
  declaration: (lexical_declaration
    "const"
    (variable_declarator
      name: (identifier) @name
      value: (_) @_value
      (#not-match? @_value "^(async\\b\\s*)?(function\\b|\\(|[A-Za-z_$][\\w$]*\\s*=>)")))
  @definition.constant)

(import_statement
  source: (string) @import.path) @import.statement

(export_statement
  source: (string) @import.path) @import.statement

(variable_declarator
  value: (call_expression
    function: (identifier) @_req
    arguments: (arguments (string) @import.path))
  (#eq? @_req "require")) @import.statement

(expression_statement
  (call_expression
    function: (identifier) @_req
    arguments: (arguments (string) @import.path)
    (#eq? @_req "require")) @import.statement)

(call_expression
  function: (identifier) @call.name
  (#not-eq? @call.name "require"))

(call_expression
  function: (member_expression
    object: (_) @call.receiver
    property: (property_identifier) @call.name))
"#,
    jsx_patterns!()
);

impl QueryHarness for JavaScriptExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_javascript::LANGUAGE.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "javascript",
            grammar_version: "0.25.0",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 3,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::JavaScript
    }

    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind> {
        match suffix {
            "function" => Some(SourceNodeKind::Function),
            "type" => Some(SourceNodeKind::Type),
            "constant" => Some(SourceNodeKind::Constant),
            _ => None,
        }
    }

    fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        if require::is_require(statement) {
            require::bindings(statement, path, site)
        } else {
            ecmascript_import_bindings(statement, path, site)
        }
    }
}

impl SourceGraphExtractor for JavaScriptExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("javascript").expect("the dialect table names every registered extractor")
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
pub(super) mod tests;
