//! Turn one query pass over a tree into owned, tree-independent data.

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, QueryMatch};

#[path = "collect_text.rs"]
mod text;

use super::binding::receiver_shadowed;
use super::{default_import_binding, QueryHarness};
use crate::context::source_graph::{body_hash, ImportBinding, SourceNodeKind, Span};
use text::{import_text, receiver_text};

/// A definition captured by the query, before scoping.
struct RawDefinition<'tree> {
    node: Node<'tree>,
    kind: SourceNodeKind,
}

/// Everything one query pass found, in source order.
#[derive(Default)]
pub(super) struct Collected {
    pub(super) definitions: Vec<Definition>,
    pub(super) imports: Vec<Reference>,
    /// Names the file's import statements bind.
    pub(super) bindings: Vec<ImportBinding>,
    pub(super) calls: Vec<Reference>,
    /// `@reference.name` uses, which become `References` edges.
    pub(super) references: Vec<Reference>,
    /// `@definition.*` matches that could not become nodes.
    pub(super) skipped: usize,
}

/// A definition with its byte range resolved, independent of the tree.
pub(super) struct Definition {
    pub(super) name: String,
    /// Segments of a `@definition.qualifier` written on the definition itself
    /// (`W` in C++ `void W::run()`); empty for most definitions.
    pub(super) qualifier: Vec<String>,
    pub(super) kind: SourceNodeKind,
    pub(super) span: Span,
    pub(super) body_hash: String,
    pub(super) signature: String,
}

/// An import path, a callee, or a referenced name, at the site it was written.
pub(super) struct Reference {
    pub(super) symbol: String,
    pub(super) site: Span,
    /// Receiver text of a member call (`self`, `obj`); `None` otherwise.
    pub(super) receiver: Option<String>,
    pub(super) receiver_shadowed: bool,
}

/// One match's captures, gathered before any is interpreted: the captures
/// that belong together (a callee and its receiver, an import path and its
/// statement, a definition and its name) arrive in no guaranteed order.
#[derive(Default)]
struct MatchCaptures<'tree> {
    definition: Option<RawDefinition<'tree>>,
    name: Option<String>,
    qualifier: Vec<String>,
    import_statement: Option<String>,
    import_paths: Vec<(String, Span)>,
    call_names: Vec<(String, Span, bool)>,
    receiver: Option<String>,
    reference_names: Vec<(String, Span)>,
    /// `@definition.*` captures whose suffix names no node kind.
    skipped: usize,
}

impl<'tree> MatchCaptures<'tree> {
    /// Record a `@definition.<suffix>` capture; any other name is ignored.
    fn definition_capture(&mut self, harness: &dyn QueryHarness, name: &str, node: Node<'tree>) {
        let Some(suffix) = name.strip_prefix("definition.") else {
            return;
        };
        match harness.kind_for_capture(suffix) {
            Some(kind) => self.definition = Some(RawDefinition { node, kind }),
            None => self.skipped += 1,
        }
    }
}

/// Walk every query match once, materializing owned data so the tree can be
/// dropped before the graph is assembled.
pub(super) fn collect(
    harness: &dyn QueryHarness,
    query: &Query,
    root: Node<'_>,
    bytes: &[u8],
) -> Collected {
    let capture_names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, root, bytes);

    let mut walk = Collected::default();
    while let Some(matched) = matches.next() {
        let found = gather(harness, capture_names, matched, bytes);
        record(harness, found, bytes, &mut walk);
    }
    walk.sort();
    walk
}

impl Collected {
    /// Canonical order: definitions by span, sites by `(start_byte, symbol)`,
    /// bindings by site and content, with exact duplicate bindings dropped.
    fn sort(&mut self) {
        self.definitions
            .sort_by_key(|definition| (definition.span.start_byte, definition.span.end_byte));
        for references in [&mut self.imports, &mut self.calls, &mut self.references] {
            references.sort_by(|a, b| {
                (a.site.start_byte, &a.symbol).cmp(&(b.site.start_byte, &b.symbol))
            });
        }
        self.bindings.sort_by(|a, b| {
            (a.site.start_byte, &a.path, &a.name, &a.alias, a.glob).cmp(&(
                b.site.start_byte,
                &b.path,
                &b.name,
                &b.alias,
                b.glob,
            ))
        });
        self.bindings.dedup();
    }
}

/// Read one match's captures into owned text and spans.
fn gather<'tree>(
    harness: &dyn QueryHarness,
    capture_names: &[&str],
    matched: &QueryMatch<'_, 'tree>,
    bytes: &[u8],
) -> MatchCaptures<'tree> {
    let mut found = MatchCaptures::default();
    for capture in matched.captures() {
        let capture_name = capture_names
            .get(capture.index as usize)
            .copied()
            .unwrap_or("");
        let node = capture.node;
        match capture_name {
            "name" => found.name = Some(node_text(node, bytes)),
            "definition.qualifier" => found
                .qualifier
                .extend(qualifier_segments(&node_text(node, bytes))),
            "import.statement" => found.import_statement = Some(import_text(harness, node, bytes)),
            "import.path" => found.import_paths.push((
                normalize_import(&import_text(harness, node, bytes)),
                span_of(node),
            )),
            "call.name" => found.call_names.push((
                normalize_call(&node_text(node, bytes)),
                span_of(node),
                receiver_shadowed(harness, node),
            )),
            "call.receiver" => found.receiver = Some(receiver_text(node, bytes)),
            "reference.name" => found
                .reference_names
                .push((normalize_call(&node_text(node, bytes)), span_of(node))),
            other => found.definition_capture(harness, other, node),
        }
    }
    found
}

/// Turn one match's captures into collected definitions, imports, bindings,
/// calls and references. A receiver without a `@call.name` in its match is
/// dropped with the match.
fn record(
    harness: &dyn QueryHarness,
    found: MatchCaptures<'_>,
    bytes: &[u8],
    walk: &mut Collected,
) {
    walk.skipped += found.skipped;
    if let Some(raw) = found.definition {
        match found.name {
            Some(name) => walk.definitions.push(materialize(
                raw,
                harness.definition_name(&name),
                found.qualifier,
                bytes,
            )),
            // An unnamed definition cannot get a stable id, so it is a
            // coverage gap, not a node.
            None => walk.skipped += 1,
        }
    }

    record_imports(
        harness,
        found.import_paths,
        found.import_statement.as_deref(),
        walk,
    );

    let receiver = found.receiver;
    walk.calls.extend(
        found
            .call_names
            .into_iter()
            .map(|(symbol, site, receiver_shadowed)| Reference {
                symbol,
                site,
                receiver: receiver.clone(),
                receiver_shadowed,
            }),
    );
    walk.references
        .extend(found.reference_names.into_iter().map(unqualified_reference));
}

fn unqualified_reference((symbol, site): (String, Span)) -> Reference {
    Reference {
        symbol,
        site,
        receiver: None,
        receiver_shadowed: false,
    }
}

/// Record each import path of a match as an `Imports` reference plus its
/// bindings: the statement's own when the match captured one, else the whole
/// module.
fn record_imports(
    harness: &dyn QueryHarness,
    import_paths: Vec<(String, Span)>,
    import_statement: Option<&str>,
    walk: &mut Collected,
) {
    for (path, site) in import_paths {
        let path = match import_statement {
            Some(statement) => {
                let spec = harness.import_spec(statement, &path);
                walk.bindings
                    .extend(harness.import_bindings(statement, &spec, site));
                spec
            }
            None => {
                walk.bindings.push(default_import_binding(&path, site));
                path
            }
        };
        walk.imports.push(Reference {
            symbol: path,
            site,
            receiver: None,
            receiver_shadowed: false,
        });
    }
}

/// Copy a captured definition out of the tree.
fn materialize(
    raw: RawDefinition<'_>,
    name: String,
    qualifier: Vec<String>,
    bytes: &[u8],
) -> Definition {
    let range = raw.node.byte_range();
    let body = &bytes[range.start..range.end];
    Definition {
        name,
        qualifier,
        kind: raw.kind,
        span: span_of(raw.node),
        signature: first_line(body),
        body_hash: body_hash(body),
    }
}

/// Id of the innermost definition whose span contains `offset`.
pub(super) fn enclosing(scopes: &[(Span, String)], offset: usize) -> Option<String> {
    scopes
        .iter()
        .filter(|(span, _)| span.start_byte <= offset && offset < span.end_byte)
        .min_by_key(|(span, _)| span.end_byte - span.start_byte)
        .map(|(_, id)| id.clone())
}

/// Byte and line span of a tree node.
fn span_of(node: Node<'_>) -> Span {
    let range = node.byte_range();
    Span {
        start_byte: range.start,
        end_byte: range.end,
        line_start: node.start_position().row + 1,
        line_end: node.end_position().row + 1,
    }
}

/// Text of a node, lossy so invalid UTF-8 never aborts an extraction.
fn node_text(node: Node<'_>, bytes: &[u8]) -> String {
    let range = node.byte_range();
    String::from_utf8_lossy(&bytes[range.start..range.end]).into_owned()
}

/// Reduce a captured callee to the spelling the graph indexes.
///
/// A qualified path is captured whole, so it can arrive wrapped across lines
/// and can carry a turbofish. Whitespace is dropped, and so is everything
/// between angle brackets: `Vec::<u8>::new` is a call to `Vec::new`, and the
/// type it is instantiated at says nothing about which definition runs. The
/// brackets are tracked by depth rather than per segment, or the `std::string`
/// inside `Vec::<std::string::String>::new` would survive the split and be read
/// as part of the path.
fn normalize_call(text: &str) -> String {
    let mut spelling = String::with_capacity(text.len());
    let mut depth = 0usize;
    for character in text.chars() {
        match character {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth > 0 || character.is_whitespace() => {}
            _ => spelling.push(character),
        }
    }
    // Dropping the arguments leaves the `::` that introduced them behind, so
    // `Vec::<u8>::new` arrives here as `Vec::::new`: an empty middle segment.
    let segments: Vec<&str> = spelling
        .split("::")
        .filter(|segment| !segment.is_empty())
        .collect();
    segments.join("::")
}

/// Segments of a `@definition.qualifier`: the text cleaned like a callee, then
/// split on `::` and `.`, so `W<T>::` and `A.B` both become scope segments.
fn qualifier_segments(text: &str) -> Vec<String> {
    normalize_call(text)
        .split([':', '.'])
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

/// Strip the quotes a grammar keeps around a string-literal import path.
fn normalize_import(text: &str) -> String {
    text.trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .to_string()
}

/// First line of a definition, trimmed — a stable, language-agnostic signature.
fn first_line(body: &[u8]) -> String {
    let end = body
        .iter()
        .position(|&byte| byte == b'\n')
        .unwrap_or(body.len());
    let text = String::from_utf8_lossy(&body[..end]);
    text.lines().next().unwrap_or("").trim().to_string()
}

/// Span and description of the first error or missing node in the tree.
pub(super) fn first_error(root: Node<'_>, bytes: &[u8]) -> (Span, String) {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    let mut best: Option<Node> = None;

    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            let replace = best
                .map(|current| node.byte_range().start < current.byte_range().start)
                .unwrap_or(true);
            if replace {
                best = Some(node);
            }
        }
        // Only descend where an error can actually be.
        if node.has_error() {
            stack.extend(node.children(&mut cursor));
        }
    }

    match best {
        Some(node) => {
            let span = span_of(node);
            let detail = format!(
                "syntax error at line {}: {}",
                span.line_start,
                first_line(node_text(node, bytes).as_bytes())
            );
            (span, detail)
        }
        None => (
            Span::default(),
            "the grammar reported an error with no locatable error node".to_string(),
        ),
    }
}
