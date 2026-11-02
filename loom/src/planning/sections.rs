//! Heading sections, anchors and span hashes of a Markdown document.
//!
//! A document splits into a preamble (the lines before its first heading) and one section per
//! ATX heading outside fenced code, each running to the next heading of the same or a shallower
//! level. Plan documents also expose their `loom METADATA` block and each stage entry inside it
//! (`metadata`); those are extra addresses over the same lines, not a re-partition.

mod metadata;
#[cfg(test)]
mod metadata_tests;
#[cfg(test)]
mod tests;

use crate::fs::knowledge::splice::{atx_heading, fence_mask, section_end, strip_closing_sequence};
use serde::Serialize;
use std::collections::HashMap;

/// A contiguous run of lines in a document with the hash of its normalised text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    /// First line of the span, 0-based.
    pub start_line: usize,
    /// One past the last line of the span, 0-based (exclusive).
    pub end_line: usize,
    /// `normalized_hash` of the span's lines joined with `\n` plus a final `\n` when non-empty.
    pub hash: String,
}

/// One ATX heading and the lines its section covers (the heading line included).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HeadingSection {
    /// Heading level, 1 to 6.
    pub level: u8,
    /// Heading text without the hashes and any closing `#` sequence.
    pub title: String,
    /// GitHub anchor, de-duplicated in document order (`slug`, `slug-1`, `slug-2`).
    pub anchor: String,
    /// The section's lines: the heading through the line before the next heading of the same
    /// or a shallower level.
    pub span: Span,
}

/// Every addressable part of a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocSections {
    /// Lines before the first heading (possibly empty); the whole text when there is none.
    pub preamble: Span,
    /// Headings in document order.
    pub headings: Vec<HeadingSection>,
    /// The `<!-- loom METADATA -->` block of a plan, start and end markers included.
    pub metadata: Option<Span>,
    /// `(stage id, span)` for every stage entry inside the metadata block, in order.
    pub stages: Vec<(String, Span)>,
    /// `normalized_hash` of the whole text.
    pub file_hash: String,
}

impl DocSections {
    /// The span an anchor addresses: `""` the preamble, `loom-metadata` the plan metadata
    /// block, `stage:<id>` a stage entry, anything else the heading with exactly that anchor.
    ///
    /// `loom-metadata` and `stage:` are reserved: a heading slugged `loom-metadata` is not
    /// reachable through `find`, so the anchor's meaning never depends on which blocks exist.
    pub fn find(&self, anchor: &str) -> Option<&Span> {
        if anchor.is_empty() {
            return Some(&self.preamble);
        }
        if anchor == "loom-metadata" {
            return self.metadata.as_ref();
        }
        if let Some(id) = anchor.strip_prefix("stage:") {
            return self
                .stages
                .iter()
                .find(|(s, _)| s == id)
                .map(|(_, span)| span);
        }
        self.headings
            .iter()
            .find(|h| h.anchor == anchor)
            .map(|h| &h.span)
    }
}

/// Split `text` into its sections, anchors and hashes.
pub fn sections_of(text: &str) -> DocSections {
    let lines = split_lines(text);
    let mask = fence_mask(&lines);
    let found = scan_headings(&lines, &mask);
    let first = found.first().map_or(lines.len(), |h| h.line);
    let mut anchors = AnchorSet::default();
    let headings = found
        .into_iter()
        .map(|h| HeadingSection {
            level: h.level,
            anchor: anchors.claim(&github_slug(h.title)),
            title: h.title.to_string(),
            span: span_of(
                &lines,
                h.line,
                section_end(&lines, &mask, h.line, h.level.into()),
            ),
        })
        .collect();
    let (metadata, stages) = metadata::metadata_spans(text, &lines);
    DocSections {
        preamble: span_of(&lines, 0, first),
        headings,
        metadata,
        stages,
        file_hash: normalized_hash(text),
    }
}

/// The GitHub anchor of a heading title, without de-duplication.
///
/// Inline Markdown is stripped first (a link `[text](url)` keeps `text`; backticks and `*` go),
/// then the text is lower-cased, every char that is not alphanumeric, a space, `-` or `_` is
/// dropped, and each space becomes `-` (runs are not collapsed).
/// `4.2 Line Grammar` -> `42-line-grammar`; `01 — Workflow Model` -> `01--workflow-model`.
pub fn github_slug(title: &str) -> String {
    strip_links(title)
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Hash `text` after line-ending and trailing-whitespace normalisation.
///
/// `\r\n` and lone `\r` become `\n`, trailing spaces and tabs are dropped from every line, and
/// whether the text ends with a newline is kept. The result is
/// `crate::context::source_graph::body_hash` of the normalised bytes (`sha256:<hex>`).
pub fn normalized_hash(text: &str) -> String {
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = unified
        .split('\n')
        .map(|line| line.trim_end_matches([' ', '\t']))
        .collect::<Vec<_>>()
        .join("\n");
    crate::context::source_graph::body_hash(normalized.as_bytes())
}

/// A heading found by `scan_headings`: its line, level and title.
struct FoundHeading<'a> {
    line: usize,
    level: u8,
    title: &'a str,
}

/// Lines of `text` split on `\n`, one trailing `\r` stripped from each. A final newline ends
/// the last line instead of starting an empty one, so a text of N lines yields N entries.
fn split_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let body = text.strip_suffix('\n').unwrap_or(text);
    body.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect()
}

/// Non-fenced ATX headings with a non-empty title, in document order. An empty heading (`##`
/// alone, or only a closing sequence) has no anchor and is skipped, though it still ends the
/// section above it (`section_end` counts every ATX heading).
fn scan_headings<'a>(lines: &[&'a str], mask: &[bool]) -> Vec<FoundHeading<'a>> {
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !mask[*i])
        .filter_map(|(line, text)| {
            let (level, raw) = atx_heading(text)?;
            let title = strip_closing_sequence(raw);
            let level = u8::try_from(level).ok()?;
            (!title.is_empty()).then_some(FoundHeading { line, level, title })
        })
        .collect()
}

/// `title` with each Markdown link `[text](url)` replaced by `text`.
fn strip_links(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(open) = rest.find('[') {
        let inner = &rest[open + 1..];
        let Some(close) = inner.find("](") else {
            break;
        };
        let Some(end) = inner[close + 2..].find(')') else {
            break;
        };
        out.push_str(&rest[..open]);
        out.push_str(&inner[..close]);
        rest = &inner[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

/// The span of `lines[start..end]` with its hash.
fn span_of(lines: &[&str], start: usize, end: usize) -> Span {
    let mut text = lines[start..end].join("\n");
    if end > start {
        text.push('\n');
    }
    Span {
        start_line: start,
        end_line: end,
        hash: normalized_hash(&text),
    }
}

/// Anchors emitted so far, de-duplicated the way GitHub does: a repeated slug takes the next
/// free `slug-N`, and every emitted anchor (generated or not) counts as taken.
#[derive(Default)]
struct AnchorSet {
    /// Emitted anchor -> how many suffixes have been tried with it as the base.
    occurrences: HashMap<String, usize>,
}

impl AnchorSet {
    /// The anchor for the next heading whose slug is `slug`.
    fn claim(&mut self, slug: &str) -> String {
        let mut anchor = slug.to_string();
        while self.occurrences.contains_key(&anchor) {
            let count = self.occurrences.entry(slug.to_string()).or_insert(0);
            *count += 1;
            anchor = format!("{slug}-{count}");
        }
        self.occurrences.insert(anchor.clone(), 0);
        anchor
    }
}
