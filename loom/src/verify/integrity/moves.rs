//! Cross-file move cancellation: an assertion statement a base test file lost
//! cancels against the same statement added to another changed test file of
//! the language when both sit in the same function (same name, same body and
//! decoration) and the runner still collects the destination. Recomputed on
//! every scan from the file contents, never stored.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::str::Lines;

use super::edits::normalize;
use super::moves_collect::Collector;
use super::statements::Statement;
use crate::context::extract::{extract_file, registry, BoxedExtractor};
use crate::context::source_graph::SourceNodeKind;
use crate::testrun::languages::LanguageProfile;
use crate::verify::contracts::collection::FileSource;

/// Longest decoration block (attribute or decorator lines) walked upward.
const MAX_BLOCK_LINES: usize = 100;

/// One changed file with a language profile: both sides' text and the
/// assertion statements each side lost or gained relative to the other.
pub(super) struct FileDelta {
    pub path: String,
    pub profile: &'static LanguageProfile,
    pub existed_at_base: bool,
    /// Lossy UTF-8 content at base, "" for a new file.
    pub base: String,
    /// Lossy UTF-8 content now, "" for a deleted file.
    pub head: String,
    /// Base statements not matched at head; offsets index `base`.
    pub lost: Vec<Statement>,
    /// Head statements not matched at base; offsets index `head`.
    pub added: Vec<Statement>,
    /// Rust only: the `.rs` paths in `path`'s directory at head.
    pub siblings: Vec<String>,
}

/// A function node: its byte span and scope (the function's name is last).
struct Node {
    start: usize,
    end: usize,
    scope: Vec<String>,
    /// [`identity`] of the node, built on first use: it walks the decoration
    /// block above the function and copies its body, and a scan compares one
    /// node against many candidates.
    identity: OnceCell<Option<String>>,
}

impl Node {
    fn identity(&self, content: &str, profile: &str) -> Option<&str> {
        self.identity
            .get_or_init(|| identity(content, self, profile))
            .as_deref()
    }
}

/// The function nodes of a delta's two sides, empty for a side with nothing
/// to match.
struct Functions {
    base: Vec<Node>,
    head: Vec<Node>,
}

/// Cancel lost statements that moved verbatim into another file the runner
/// collects; `head` reads the head tree. Returns, per file whose statements
/// were cancelled, the paths of the files that cancelled them.
pub(super) fn cancel_moves(
    deltas: &mut [FileDelta],
    head: FileSource<'_>,
) -> BTreeMap<String, BTreeSet<String>> {
    cancel_with(&registry(), deltas, head)
}

/// [`cancel_moves`] over an explicit extractor list; none cancels nothing.
fn cancel_with(
    extractors: &[BoxedExtractor],
    deltas: &mut [FileDelta],
    head: FileSource<'_>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut partners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    if extractors.is_empty() {
        return partners;
    }
    let functions: Vec<Functions> = deltas
        .iter()
        .enumerate()
        .map(|(index, delta)| extract_functions(extractors, deltas, index, delta))
        .collect();
    let mut collector = Collector::new(head);
    let mut lost_gone = vec![BTreeSet::new(); deltas.len()];
    let mut added_gone: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); deltas.len()];
    for source in 0..deltas.len() {
        for (index, statement) in deltas[source].lost.iter().enumerate() {
            let found = find_partner(
                deltas,
                &functions,
                &added_gone,
                &mut collector,
                source,
                statement,
            );
            if let Some((dest, added)) = found {
                lost_gone[source].insert(index);
                added_gone[dest].insert(added);
                partners
                    .entry(deltas[source].path.clone())
                    .or_default()
                    .insert(deltas[dest].path.clone());
            }
        }
    }
    for (delta, (lost, added)) in deltas.iter_mut().zip(lost_gone.iter().zip(&added_gone)) {
        retain_unmatched(&mut delta.lost, lost);
        retain_unmatched(&mut delta.added, added);
    }
    partners
}

fn retain_unmatched(statements: &mut Vec<Statement>, gone: &BTreeSet<usize>) {
    let mut index = 0;
    statements.retain(|_| {
        index += 1;
        !gone.contains(&(index - 1))
    });
}

/// The destination delta and added-statement index that cancel `statement`
/// of `deltas[source]`: same key, same-named function with an identical
/// identity, in a file of the same language that the runner collects.
fn find_partner(
    deltas: &[FileDelta],
    functions: &[Functions],
    taken: &[BTreeSet<usize>],
    collector: &mut Collector<'_>,
    source: usize,
    statement: &Statement,
) -> Option<(usize, usize)> {
    let from = &deltas[source];
    let node = innermost(&functions[source].base, statement.start)?;
    let wanted = node.identity(&from.base, from.profile.name)?;
    for (index, dest) in deltas.iter().enumerate() {
        if index == source || dest.profile.name != from.profile.name {
            continue;
        }
        for (added, candidate) in dest.added.iter().enumerate() {
            if candidate.key != statement.key || taken[index].contains(&added) {
                continue;
            }
            let Some(found) = innermost(&functions[index].head, candidate.start) else {
                continue;
            };
            let same = found.scope.last() == node.scope.last()
                && found.identity(&dest.head, dest.profile.name) == Some(wanted);
            if same && collector.collected(dest, &found.scope) {
                return Some((index, added));
            }
        }
    }
    None
}

/// The smallest function node whose span holds `offset`.
fn innermost(nodes: &[Node], offset: usize) -> Option<&Node> {
    nodes
        .iter()
        .filter(|node| node.start <= offset && offset < node.end)
        .min_by_key(|node| node.end - node.start)
}

/// The symbol-level function nodes of the sides of `delta` (`deltas[at]`)
/// that can take part in a match; a file the extractors cannot read symbol by
/// symbol (no grammar, parse error, oversized) has none. A side matters only
/// when it has statements and another delta of the language holds the
/// opposite kind: lost statements pair with added ones in other files.
fn extract_functions(
    extractors: &[BoxedExtractor],
    deltas: &[FileDelta],
    at: usize,
    delta: &FileDelta,
) -> Functions {
    let elsewhere = |holds: fn(&FileDelta) -> bool| {
        deltas.iter().enumerate().any(|(index, other)| {
            index != at && other.profile.name == delta.profile.name && holds(other)
        })
    };
    let nodes = |content: &str, wanted: bool| {
        if !wanted || content.is_empty() {
            return Vec::new();
        }
        extract_file(extractors, Path::new(&delta.path), content.as_bytes())
            .nodes
            .into_iter()
            .filter(|node| node.kind == SourceNodeKind::Function && node.coverage.has_symbols())
            .filter(|node| !node.scope.is_empty())
            .map(|node| Node {
                start: node.span.start_byte,
                end: node.span.end_byte,
                scope: node.scope,
                identity: OnceCell::new(),
            })
            .collect()
    };
    Functions {
        base: nodes(
            &delta.base,
            !delta.lost.is_empty() && elsewhere(|other| !other.added.is_empty()),
        ),
        head: nodes(
            &delta.head,
            !delta.added.is_empty() && elsewhere(|other| !other.lost.is_empty()),
        ),
    }
}

/// What makes two functions the same test: the normalized decoration prefix
/// (attributes and decorators above the span), the line-head text before the
/// span (a JS `it.skip(` wrapper), and the body. `None` when the span does
/// not index `content`.
fn identity(content: &str, node: &Node, profile: &str) -> Option<String> {
    let line_start = content
        .get(..node.start)?
        .rfind('\n')
        .map_or(0, |at| at + 1);
    let prefix = decoration_prefix(&content[..line_start]);
    let text = content.get(line_start..node.end)?;
    Some(normalize(&format!("{prefix}\n{text}"), profile))
}

/// The decoration lines above a function, as a block: lines whose text starts
/// with `#[`, `#![`, `@` or is a bare `[Name..]` attribute, and a line closing
/// a bracket opened above, which pulls in the lines up to the one opening it
/// (a multi-line `#[cfg_attr(..)]`, a black-formatted
/// `@pytest.mark.parametrize(..)`) when that line is itself a decoration.
/// Blank lines and comment lines between decorations, or between the
/// decorations and the function, are walked over and left out of the block;
/// any other code line ends the walk.
fn decoration_prefix(above: &str) -> String {
    let mut lines = Upward::new(above);
    let mut blocks: Vec<String> = Vec::new();
    let mut top = 0;
    while let Some(line) = lines.get(top) {
        let text = line.trim();
        if text.is_empty() || is_comment(text) {
            top += 1;
            continue;
        }
        let (closers, openers) = brackets(text);
        let start = if closers > openers {
            block_start(&mut lines, top)
        } else if is_decoration(text) {
            Some(top)
        } else {
            None
        };
        let Some(start) = start else { break };
        blocks.push(lines.block(top, start));
        top = start + 1;
    }
    blocks.reverse();
    blocks.join("\n")
}

/// The lines above a function read from the nearest one upward, so a walk
/// that ends at the first code line never touches the rest of the file.
struct Upward<'a> {
    rest: std::iter::Rev<Lines<'a>>,
    seen: Vec<&'a str>,
}

impl<'a> Upward<'a> {
    fn new(above: &'a str) -> Self {
        Self {
            rest: above.lines().rev(),
            seen: Vec::new(),
        }
    }

    /// The line `distance` lines above the function (0 is the nearest).
    fn get(&mut self, distance: usize) -> Option<&'a str> {
        while self.seen.len() <= distance {
            self.seen.push(self.rest.next()?);
        }
        Some(self.seen[distance])
    }

    /// Lines `from..=to` (distances), in source order.
    fn block(&self, from: usize, to: usize) -> String {
        let mut block: Vec<&str> = self.seen[from..=to].to_vec();
        block.reverse();
        block.join("\n")
    }
}

/// A whole-line comment: `//`, `#` (not an attribute `#[` or `#!`) or a
/// single-line `/* .. */`.
fn is_comment(text: &str) -> bool {
    text.starts_with("//")
        || (text.starts_with('#') && !text.starts_with("#[") && !text.starts_with("#!"))
        || (text.starts_with("/*") && text.ends_with("*/"))
}

/// The distance of the line opening the bracket group that the line at
/// distance `from` closes, when that line is a decoration and the group is at
/// most [`MAX_BLOCK_LINES`] long.
fn block_start(lines: &mut Upward<'_>, from: usize) -> Option<usize> {
    let mut depth: i64 = 0;
    for distance in from..from + MAX_BLOCK_LINES {
        let line = lines.get(distance)?.trim();
        let (closers, openers) = brackets(line);
        depth += closers as i64 - openers as i64;
        if depth <= 0 {
            return is_decoration(line).then_some(distance);
        }
    }
    None
}

fn is_decoration(text: &str) -> bool {
    text.starts_with("#[")
        || text.starts_with("#![")
        || text.starts_with('@')
        || (text.starts_with('[')
            && text.ends_with(']')
            && text[1..].starts_with(|ch: char| ch.is_ascii_alphabetic()))
}

/// The closing and opening brackets of `text` outside double-quoted strings.
fn brackets(text: &str) -> (usize, usize) {
    let (mut closers, mut openers) = (0, 0);
    let (mut quoted, mut escaped) = (false, false);
    for ch in text.chars() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
        } else {
            match ch {
                '"' => quoted = true,
                ')' | ']' | '}' => closers += 1,
                '(' | '[' | '{' => openers += 1,
                _ => {}
            }
        }
    }
    (closers, openers)
}

#[cfg(all(test, feature = "source-graph"))]
#[path = "moves_tests.rs"]
mod tests;
