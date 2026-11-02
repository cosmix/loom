use std::path::Path;

use anyhow::Result;

use crate::context::extract::c::{
    c_family_query, c_prototype_shapes, include_bindings, prototypes_under,
};
use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

/// Extracts declarations, `#include`s and calls from C++ sources and `.h`
/// headers. A prototype or in-class `void run();` is not a definition:
/// `@definition.function` sits on the `function_definition`, so its span covers
/// the body. A free prototype (`int total(const Shape &s);`) is a `References`
/// edge naming the function, as in C. A qualified definition (`void W::run() {}`) scopes as `[W, run]`,
/// and a `namespace A::B` is one `Module` with the single scope segment
/// `A::B`. Templates, overload resolution and macros are gaps: a template's
/// inner definition is captured as written and no instantiation is modelled.
pub struct CppExtractor;

impl CppExtractor {
    pub fn new() -> Self {
        CppExtractor
    }
}

impl Default for CppExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// The name of a definition, as written after any qualifier.
macro_rules! cpp_name {
    () => {
        "[(identifier) (destructor_name) (operator_name)] @name"
    };
}

/// A qualified declarator `Q` in every declarator shape a function definition
/// can take: plain, returning a pointer, returning a reference.
macro_rules! function_with_declarator {
    ($declarator:expr) => {
        concat!(
            "(function_definition\n  declarator: [\n    (function_declarator declarator: ",
            $declarator,
            ")\n    (pointer_declarator declarator: (function_declarator declarator: ",
            $declarator,
            "))\n    (reference_declarator (function_declarator declarator: ",
            $declarator,
            "))\n  ]\n  body: (compound_statement)) @definition.function\n\n"
        )
    };
}

/// The C prototype shapes plus one returning a reference.
macro_rules! cpp_prototype_shapes {
    () => {
        concat!(
            c_prototype_shapes!(),
            r#"
      (reference_declarator
        (function_declarator
          declarator: (identifier) @reference.name))"#
        )
    };
}

/// `A::B::run` parses as a `qualified_identifier` nested in the `name` of
/// another, so each level of nesting adds one `@definition.qualifier` scope.
/// The qualifiers of one match are inserted into the scope in tree order, so
/// `void A::B::run() {}` scopes as `[A, B, run]`. Four levels are matched; a
/// deeper qualifier is a gap.
macro_rules! qualified_declarator {
    ($name:expr) => {
        concat!(
            "(qualified_identifier scope: (_) @definition.qualifier name: ",
            $name,
            ")"
        )
    };
}

const QUERY: &str = concat!(
    // In-class methods and free functions: a bare or field name. A definition
    // with no body (`void run();`, `= default`) is not a definition.
    r#"
(function_definition
  declarator: [
    (function_declarator
      declarator: [(identifier) (field_identifier) (destructor_name) (operator_name)] @name)
    (pointer_declarator
      declarator: (function_declarator
        declarator: [(identifier) (field_identifier) (destructor_name) (operator_name)] @name))
    (reference_declarator
      (function_declarator
        declarator: [(identifier) (field_identifier) (destructor_name) (operator_name)] @name))
  ]
  body: (compound_statement)) @definition.function

"#,
    function_with_declarator!(qualified_declarator!(cpp_name!())),
    function_with_declarator!(qualified_declarator!(qualified_declarator!(cpp_name!()))),
    function_with_declarator!(qualified_declarator!(qualified_declarator!(
        qualified_declarator!(cpp_name!())
    ))),
    function_with_declarator!(qualified_declarator!(qualified_declarator!(
        qualified_declarator!(qualified_declarator!(cpp_name!()))
    ))),
    r#"
(class_specifier
  name: (type_identifier) @name
  body: (_)) @definition.type

(alias_declaration
  name: (type_identifier) @name) @definition.type

(namespace_definition
  name: [(namespace_identifier) (nested_namespace_specifier)] @name) @definition.module

(call_expression
  function: (qualified_identifier) @call.name)

(call_expression
  function: (template_function
    name: (identifier) @call.name))

(call_expression
  function: (field_expression
    argument: (_) @call.receiver
    field: (template_method
      name: (field_identifier) @call.name)))
"#,
    // A template or friend declaration of a function is a prototype too; the
    // shared patterns cover every other parent.
    prototypes_under!(cpp_prototype_shapes!(); "template_declaration", "friend_declaration"),
    c_family_query!(cpp_prototype_shapes!())
);

impl QueryHarness for CppExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_cpp::LANGUAGE.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "cpp",
            grammar_version: "0.23.4",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 4,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::Cpp
    }

    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind> {
        match suffix {
            "function" => Some(SourceNodeKind::Function),
            "type" => Some(SourceNodeKind::Type),
            "module" => Some(SourceNodeKind::Module),
            _ => None,
        }
    }

    fn import_bindings(&self, _statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        include_bindings(path, site)
    }
}

impl SourceGraphExtractor for CppExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("cpp").expect("the dialect table names every registered extractor")
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
