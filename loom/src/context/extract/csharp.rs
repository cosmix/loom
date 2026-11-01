use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

/// Extracts declarations, usings, and calls from `.cs` files. A method without a body
/// (abstract, interface, extern) is not a definition.
pub struct CSharpExtractor;

impl CSharpExtractor {
    pub fn new() -> Self {
        CSharpExtractor
    }
}

impl Default for CSharpExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(namespace_declaration
  name: [(identifier) (qualified_name)] @name) @definition.module

(file_scoped_namespace_declaration
  name: [(identifier) (qualified_name)] @name) @definition.module

(class_declaration
  name: (identifier) @name) @definition.type

(struct_declaration
  name: (identifier) @name) @definition.type

(record_declaration
  name: (identifier) @name) @definition.type

(enum_declaration
  name: (identifier) @name) @definition.type

(interface_declaration
  name: (identifier) @name) @definition.interface

(method_declaration
  name: (identifier) @name
  body: (_)) @definition.function

(constructor_declaration
  name: (identifier) @name
  body: (_)) @definition.function

(local_function_statement
  name: (identifier) @name
  body: (_)) @definition.function

; `using A.B;`, `using static A.B.C;` and `using X = A.B.C;` carry the path as
; the directive's unnamed type child. In the alias form the `name:` field is the
; alias, never the path, so a lone identifier path is matched by shape.
(using_directive
  (qualified_name) @import.path) @import.statement

(using_directive
  !name
  (identifier) @import.path) @import.statement

(using_directive
  name: (identifier)
  (identifier) @import.path) @import.statement

(invocation_expression
  function: (identifier) @call.name)

(invocation_expression
  function: (generic_name (identifier) @call.name))

; `this` and `base` are anonymous tokens in the grammar, so `(_)` alone misses them.
(invocation_expression
  function: (member_access_expression
    expression: [(_) "this" "base"] @call.receiver
    name: (identifier) @call.name))

(invocation_expression
  function: (member_access_expression
    expression: [(_) "this" "base"] @call.receiver
    name: (generic_name (identifier) @call.name)))

; `a?.M()` keeps its receiver on the conditional access that is the callee.
(invocation_expression
  function: (conditional_access_expression
    condition: [(_) "this" "base"] @call.receiver
    (member_binding_expression
      name: (identifier) @call.name)))

; `new W()` calls `W`'s constructor, which is named after the class.
(object_creation_expression
  type: (identifier) @call.name)

(object_creation_expression
  type: (generic_name (identifier) @call.name))

(object_creation_expression
  type: (qualified_name name: (identifier) @call.name))
"#;

impl QueryHarness for CSharpExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_c_sharp::LANGUAGE.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "csharp",
            grammar_version: "0.23.5",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 3,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::CSharp
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

    /// `using A.B;` and `using static A.B.C;` bind every member of the path;
    /// `using X = A.B.C;` binds `C` under the alias `X`.
    fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        let alias = statement
            .trim_end_matches(';')
            .split_once('=')
            .and_then(|(head, _)| head.split_whitespace().last())
            .map(str::to_string);
        let name = alias
            .is_some()
            .then(|| path.rsplit('.').next().unwrap_or(path).to_string());
        vec![ImportBinding {
            path: path.to_string(),
            glob: alias.is_none(),
            name,
            alias,
            exported_as: None,
            site,
        }]
    }
}

impl SourceGraphExtractor for CSharpExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("csharp").expect("the dialect table names every registered extractor")
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
#[path = "csharp/tests.rs"]
mod tests;
