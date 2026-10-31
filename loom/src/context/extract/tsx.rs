use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::ecmascript::jsx_patterns;
use crate::context::extract::typescript::ecmascript_import_bindings;
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

/// Extracts definitions, imports, calls and JSX element uses from `.tsx` files
/// with the separately pinned `LANGUAGE_TSX` grammar.
pub struct TsxExtractor;

impl TsxExtractor {
    pub fn new() -> Self {
        TsxExtractor
    }
}

impl Default for TsxExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// TSX declaration, import and call patterns, then the JSX patterns. The
/// TypeScript-only constructs (interfaces, type aliases, enums, namespaces,
/// abstract classes, `type_identifier` class names) match the TypeScript query;
/// function-valued declarations follow the JavaScript query, so `@definition`
/// always covers the whole declaration and a `.tsx` component spans the same
/// text a `.jsx` one does.
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

(internal_module
  name: (identifier) @name) @definition.module

(export_statement
  declaration: (lexical_declaration
    "const"
    (variable_declarator
      name: (identifier) @name
      value: (_) @_value
      (#not-match? @_value "^(async\\b\\s*)?(function\\b|\\(|<\\s*[A-Za-z_$][\\w$]*\\s*(,|extends\\b)|[A-Za-z_$][\\w$]*\\s*=>)")))
  @definition.constant)

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
"#,
    jsx_patterns!()
);

impl QueryHarness for TsxExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "tsx",
            grammar_version: "0.23.2",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 3,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::Tsx
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

impl SourceGraphExtractor for TsxExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("tsx").expect("the dialect table names every registered extractor")
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
