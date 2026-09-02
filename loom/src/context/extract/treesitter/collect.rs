//! Turn one query pass over a tree into owned, tree-independent data.

use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor, QueryMatch};

use super::QueryHarness;
use crate::context::source_graph::{SourceNodeKind, Span};

/// A definition captured by the query, before scoping.
struct RawDefinition<'tree> {
    node: Node<'tree>,
    name: String,
    kind: SourceNodeKind,
}

/// Everything one query pass found, in source order.
pub(super) struct Collected {
    pub(super) definitions: Vec<Definition>,
    pub(super) imports: Vec<Reference>,
    pub(super) calls: Vec<Reference>,
    /// `@definition.*` matches that could not become nodes.
    pub(super) skipped: usize,
}

/// A definition with its byte range resolved, independent of the tree.
pub(super) struct Definition {
    pub(super) name: String,
    pub(super) kind: SourceNodeKind,
    pub(super) span: Span,
    pub(super) body: Vec<u8>,
    pub(super) signature: String,
}

/// An import path or a callee identifier, with the byte offset it appeared at.
pub(super) struct Reference {
    pub(super) symbol: String,
    pub(super) at_byte: usize,
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

    let mut definitions = Vec::new();
    let mut imports = Vec::new();
    let mut calls = Vec::new();
    let mut skipped = 0usize;

    while let Some(matched) = matches.next() {
        let (definition, name, match_skipped) = process_match(
            harness,
            capture_names,
            matched,
            bytes,
            &mut imports,
            &mut calls,
        );
        skipped += match_skipped;

        if let Some(mut raw) = definition {
            match name {
                Some(name) => {
                    raw.name = name;
                    definitions.push(materialize(raw, bytes));
                }
                // An unnamed definition cannot get a stable id, so it is a
                // coverage gap, not a node.
                None => skipped += 1,
            }
        }
    }

    definitions.sort_by_key(|definition| (definition.span.start_byte, definition.span.end_byte));
    imports.sort_by(|a, b| (a.at_byte, &a.symbol).cmp(&(b.at_byte, &b.symbol)));
    calls.sort_by(|a, b| (a.at_byte, &a.symbol).cmp(&(b.at_byte, &b.symbol)));

    Collected {
        definitions,
        imports,
        calls,
        skipped,
    }
}

/// Process one match's captures: imports and calls are complete as soon as
/// they are seen, so they are recorded directly, while a definition capture
/// is only resolved once the whole match's `@name` (if any) is known.
fn process_match<'tree>(
    harness: &dyn QueryHarness,
    capture_names: &[&str],
    matched: &QueryMatch<'_, 'tree>,
    bytes: &[u8],
    imports: &mut Vec<Reference>,
    calls: &mut Vec<Reference>,
) -> (Option<RawDefinition<'tree>>, Option<String>, usize) {
    let mut definition: Option<RawDefinition> = None;
    let mut name: Option<String> = None;
    let mut skipped = 0usize;

    for capture in matched.captures() {
        let capture_name = capture_names
            .get(capture.index as usize)
            .copied()
            .unwrap_or("");
        let text = node_text(capture.node, bytes);

        match capture_name {
            "name" => name = Some(text),
            "import.path" => imports.push(Reference {
                symbol: normalize_import(&text),
                at_byte: capture.node.byte_range().start,
            }),
            "call.name" => calls.push(Reference {
                symbol: normalize_call(&text),
                at_byte: capture.node.byte_range().start,
            }),
            other => {
                if let Some(suffix) = other.strip_prefix("definition.") {
                    match harness.kind_for_capture(suffix) {
                        Some(kind) => {
                            definition = Some(RawDefinition {
                                node: capture.node,
                                name: String::new(),
                                kind,
                            })
                        }
                        None => skipped += 1,
                    }
                }
            }
        }
    }

    (definition, name, skipped)
}

/// Copy a captured definition out of the tree.
fn materialize(raw: RawDefinition<'_>, bytes: &[u8]) -> Definition {
    let range = raw.node.byte_range();
    let body = bytes[range.start..range.end].to_vec();
    Definition {
        name: raw.name,
        kind: raw.kind,
        span: span_of(raw.node),
        signature: first_line(&body),
        body,
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

/// Strip the quotes a grammar keeps around a string-literal import path.
fn normalize_import(text: &str) -> String {
    text.trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .to_string()
}

/// First line of a definition, trimmed — a stable, language-agnostic signature.
fn first_line(body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
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
