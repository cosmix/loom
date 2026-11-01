use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

/// Extracts declarations, imports, and calls from `.java` files. A method without a body
/// (abstract or interface) is not a definition.
pub struct JavaExtractor;

impl JavaExtractor {
    pub fn new() -> Self {
        JavaExtractor
    }
}

impl Default for JavaExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(package_declaration
  [(scoped_identifier) (identifier)] @name) @definition.module

(class_declaration
  name: (identifier) @name) @definition.type

(record_declaration
  name: (identifier) @name) @definition.type

(enum_declaration
  name: (identifier) @name) @definition.type

(interface_declaration
  name: (identifier) @name) @definition.interface

(annotation_type_declaration
  name: (identifier) @name) @definition.interface

(method_declaration
  name: (identifier) @name
  body: (block)) @definition.function

(constructor_declaration
  name: (identifier) @name) @definition.function

(compact_constructor_declaration
  name: (identifier) @name) @definition.function

; `import a.b.C;`, `import static a.b.C.m;` and `import a.b.*;` (the path is
; the name before `.*`).
(import_declaration
  [(scoped_identifier) (identifier)] @import.path) @import.statement

; A bare call names a member of the enclosing class or an outer one.
(method_invocation
  !object
  name: (identifier) @call.name)

(method_invocation
  object: (_) @call.receiver
  name: (identifier) @call.name)

; `new W()` calls `W`'s constructor, which is named after the class.
(object_creation_expression
  type: (type_identifier) @call.name)

(object_creation_expression
  type: (generic_type (type_identifier) @call.name))
"#;

impl QueryHarness for JavaExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "java",
            grammar_version: "0.23.5",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 3,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::Java
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

    /// `import a.b.C;` and `import static a.b.C.m;` bind their last segment;
    /// `import a.b.*;` (the path is `a.b`) binds every member of the package.
    fn import_bindings(&self, statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        let glob = statement.trim_end_matches(';').trim_end().ends_with('*');
        let name = (!glob).then(|| path.rsplit('.').next().unwrap_or(path).to_string());
        vec![ImportBinding {
            path: path.to_string(),
            name,
            alias: None,
            glob,
            exported_as: None,
            site,
        }]
    }
}

impl SourceGraphExtractor for JavaExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("java").expect("the dialect table names every registered extractor")
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
#[path = "java/tests.rs"]
mod tests;
