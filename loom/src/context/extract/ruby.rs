use std::path::Path;

use anyhow::Result;

use crate::context::extract::dialect::{dialect_by_id, DialectSpec};
use crate::context::extract::{
    run_query, Capabilities, ExtractorIdentity, FileExtraction, QueryHarness, SourceGraphExtractor,
};
use crate::context::source_graph::{ImportBinding, NodeLanguage, SourceNodeKind, Span};

/// Extracts declarations from `.rb`, `.rake` and `.gemspec` files.
pub struct RubyExtractor;

impl RubyExtractor {
    pub fn new() -> Self {
        RubyExtractor
    }
}

impl Default for RubyExtractor {
    fn default() -> Self {
        Self::new()
    }
}

const QUERY: &str = r#"
(method
  name: (_) @name) @definition.function

(singleton_method
  name: (_) @name) @definition.function

(class
  name: (constant) @name) @definition.type

(class
  name: (scope_resolution
    scope: (_)? @definition.qualifier
    name: (_) @name)) @definition.type

(module
  name: (constant) @name) @definition.module

(module
  name: (scope_resolution
    scope: (_)? @definition.qualifier
    name: (_) @name)) @definition.module

; A receiverless `require`, `require_relative` or `load` of a plain string.
; The method name is matched here and read again from the statement text by
; `import_spec`. An interpolated string has more than one child, so it is no
; import.
((call
  !receiver
  method: (identifier) @_m
  arguments: (argument_list
    .
    (string
      .
      (string_content) @import.path
      .))) @import.statement
  (#match? @_m "^(require|require_relative|load)$"))

; A bare `name` with no receiver and no arguments parses as an identifier, not
; a call, so it is never captured. Class-body macros are not calls to a member.
((call
  !receiver
  method: (identifier) @call.name)
  (#not-match? @call.name "^(require|require_relative|load|attr_accessor|attr_reader|attr_writer|include|extend)$"))

(call
  receiver: (_) @call.receiver
  method: (identifier) @call.name)
"#;

impl QueryHarness for RubyExtractor {
    fn language(&self) -> tree_sitter::Language {
        tree_sitter_ruby::LANGUAGE.into()
    }

    fn query_source(&self) -> &'static str {
        QUERY
    }

    fn identity(&self) -> ExtractorIdentity {
        ExtractorIdentity {
            dialect: "ruby",
            grammar_version: "0.23.1",
            query_digest: crate::context::source_graph::body_hash(QUERY.as_bytes()),
            extractor_version: 2,
        }
    }

    fn node_language(&self) -> NodeLanguage {
        NodeLanguage::Ruby
    }

    fn kind_for_capture(&self, suffix: &str) -> Option<SourceNodeKind> {
        match suffix {
            "function" => Some(SourceNodeKind::Function),
            "type" => Some(SourceNodeKind::Type),
            "module" => Some(SourceNodeKind::Module),
            _ => None,
        }
    }

    /// `require_relative 'x'` is the spec `./x`; a spec already written `./` or
    /// `../` stays as written, and `require`/`load` keep the spec verbatim.
    fn import_spec(&self, statement: &str, path: &str) -> String {
        let relative = statement.trim_start().starts_with("require_relative");
        if relative && !path.starts_with("./") && !path.starts_with("../") {
            format!("./{path}")
        } else {
            path.to_string()
        }
    }

    /// A top-level `self` is the `main` object, whose methods are the file's
    /// top-level `def`s, so `self.run` there binds like a bare `run`.
    fn top_level_self(&self) -> bool {
        true
    }

    /// A require makes everything the required file defines visible, so it
    /// binds no local name: one glob binding.
    fn import_bindings(&self, _statement: &str, path: &str, site: Span) -> Vec<ImportBinding> {
        vec![ImportBinding {
            path: path.to_string(),
            name: None,
            alias: None,
            glob: true,
            exported_as: None,
            site,
        }]
    }
}

impl SourceGraphExtractor for RubyExtractor {
    fn dialect(&self) -> &'static DialectSpec {
        dialect_by_id("ruby").expect("the dialect table names every registered extractor")
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
mod tests {
    use std::path::Path;

    use crate::context::source_graph::{
        EdgeProvenance, FileCoverage, SourceEdge, SourceEdgeKind, SourceNodeKind, UNRESOLVED_TARGET,
    };

    use super::*;

    const NESTED: &str = include_str!("../../../tests/fixtures/source/ruby/nested.rb");
    const REOPENED: &str = include_str!("../../../tests/fixtures/source/ruby/reopened.rb");
    const REQUIRES: &str = include_str!("../../../tests/fixtures/source/ruby/requires.rb");
    const SYNTAX_ERROR: &str = include_str!("../../../tests/fixtures/source/ruby/syntax_error.rb");

    fn extract(path: &str, source: &str) -> FileExtraction {
        RubyExtractor::new()
            .extract(Path::new(path), source.as_bytes())
            .unwrap()
    }

    fn call<'a>(extraction: &'a FileExtraction, symbol: &str) -> &'a SourceEdge {
        let calls: Vec<_> = extraction
            .edges
            .iter()
            .filter(|edge| edge.kind == SourceEdgeKind::Calls && edge.symbol == symbol)
            .collect();
        assert_eq!(calls.len(), 1, "calls to {symbol}: {:#?}", extraction.edges);
        calls[0]
    }

    fn sorted_ids(extraction: &FileExtraction) -> Vec<&str> {
        let mut ids: Vec<&str> = extraction.nodes.iter().map(|n| n.id.as_str()).collect();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn extracts_declaration_ids_with_full_coverage() {
        let extraction = extract("lib/widget.rb", "class Widget\n  def run\n  end\nend\n");

        assert_eq!(
            sorted_ids(&extraction),
            vec![
                "lib/widget.rb",
                "lib/widget.rb#function:Widget::run",
                "lib/widget.rb#type:Widget"
            ]
        );
        assert_eq!(extraction.coverage, FileCoverage::Full);
    }

    #[test]
    fn modules_and_a_qualified_class_name_scope_their_members() {
        let extraction = extract("lib/nested.rb", NESTED);

        assert_eq!(extraction.coverage, FileCoverage::Full);
        assert_eq!(
            sorted_ids(&extraction),
            vec![
                "lib/nested.rb",
                "lib/nested.rb#function:Outer::Inner::Deep::value",
                "lib/nested.rb#function:Outer::Inner::Widget::finish",
                "lib/nested.rb#function:Outer::Inner::Widget::prepare",
                "lib/nested.rb#function:Outer::Inner::Widget::run",
                "lib/nested.rb#module:Outer",
                "lib/nested.rb#module:Outer::Inner",
                "lib/nested.rb#type:Outer::Inner::Deep",
                "lib/nested.rb#type:Outer::Inner::Widget",
            ]
        );
    }

    #[test]
    fn a_self_receiver_binds_the_member_and_a_bare_call_binds_by_scope() {
        let extraction = extract("lib/nested.rb", NESTED);
        let run = "lib/nested.rb#function:Outer::Inner::Widget::run";

        let prepare = call(&extraction, "prepare");
        assert_eq!(prepare.from, run);
        assert_eq!(
            prepare.to,
            "lib/nested.rb#function:Outer::Inner::Widget::prepare"
        );
        assert_eq!(prepare.provenance, EdgeProvenance::Receiver);
        assert_eq!(prepare.receiver.as_deref(), Some("self"));
        assert_eq!(prepare.sites.len(), 1);
        assert_eq!(prepare.sites[0].line_start, 5);

        let finish = call(&extraction, "finish");
        assert_eq!(finish.from, run);
        assert_eq!(
            finish.to,
            "lib/nested.rb#function:Outer::Inner::Widget::finish"
        );
        assert_eq!(finish.provenance, EdgeProvenance::LocalName);
        assert_eq!(finish.receiver, None);
    }

    #[test]
    fn a_dynamic_or_constant_receiver_stays_unresolved_with_its_text() {
        let extraction = extract("lib/nested.rb", NESTED);

        for (symbol, receiver, line) in [("call", "target", 7), ("build", "Helper", 8)] {
            let edge = call(&extraction, symbol);
            assert_eq!(edge.to, UNRESOLVED_TARGET, "edge: {edge:#?}");
            assert_eq!(edge.provenance, EdgeProvenance::Syntax, "edge: {edge:#?}");
            assert_eq!(edge.receiver.as_deref(), Some(receiver), "edge: {edge:#?}");
            assert_eq!(edge.sites[0].line_start, line, "edge: {edge:#?}");
        }
    }

    #[test]
    fn a_reopened_class_gets_distinct_ids_sharing_one_symbol_key() {
        let extraction = extract("lib/widget.rb", REOPENED);

        let widgets: Vec<_> = extraction
            .nodes
            .iter()
            .filter(|node| node.kind == SourceNodeKind::Type)
            .collect();
        assert_eq!(widgets.len(), 2, "nodes: {:#?}", extraction.nodes);
        assert_ne!(widgets[0].id, widgets[1].id);
        for widget in widgets {
            assert!(widget.id.starts_with("lib/widget.rb#type:Widget@"));
            assert_eq!(widget.symbol_key, "lib/widget.rb#type:Widget");
        }
        let ids = sorted_ids(&extraction);
        assert!(ids.contains(&"lib/widget.rb#function:Widget::one"));
        assert!(ids.contains(&"lib/widget.rb#function:Widget::two"));
    }

    #[test]
    fn require_relative_and_require_keep_distinct_specs() {
        let extraction = extract("lib/a.rb", "require_relative 'x'\nrequire 'x'\n");

        let paths: Vec<&str> = extraction
            .imports
            .iter()
            .map(|binding| binding.path.as_str())
            .collect();
        assert_eq!(paths, vec!["./x", "x"]);
        let edge_symbols: Vec<&str> = extraction
            .edges
            .iter()
            .filter(|edge| edge.kind == SourceEdgeKind::Imports)
            .map(|edge| edge.symbol.as_str())
            .collect();
        assert_eq!(edge_symbols, vec!["./x", "x"]);
    }

    #[test]
    fn every_require_form_is_one_glob_binding_and_never_a_call() {
        let extraction = extract("lib/a.rb", REQUIRES);

        let bindings: Vec<_> = extraction
            .imports
            .iter()
            .map(|b| {
                (
                    b.path.as_str(),
                    b.glob,
                    b.alias.as_deref(),
                    b.site.line_start,
                )
            })
            .collect();
        assert_eq!(
            bindings,
            vec![
                ("json", true, None, 1),
                ("./helper", true, None, 2),
                ("../lib/util", true, None, 3),
                ("./sibling", true, None, 4),
                ("tasks.rb", true, None, 5),
            ]
        );
        assert!(extraction.imports.iter().all(|b| b.local_name().is_none()));
        assert!(
            !extraction
                .edges
                .iter()
                .any(|e| e.kind == SourceEdgeKind::Calls),
            "edges: {:#?}",
            extraction.edges
        );
    }

    #[test]
    fn a_syntax_error_yields_a_parse_error_and_no_symbols() {
        let extraction = extract("lib/broken.rb", SYNTAX_ERROR);

        assert!(
            matches!(extraction.coverage, FileCoverage::ParseError { .. }),
            "coverage: {:?}",
            extraction.coverage
        );
        assert_eq!(sorted_ids(&extraction), vec!["lib/broken.rb"]);
    }
}
