//! Settled marks: the `Settled:` line a reader's agreement writes into a document.
//!
//! `Settled: <body> [stub <stub>] [upstream <upstream>]`, each hash the first 12 lowercase hex
//! digits of a SHA-256 (`short_hash`). The line is its own paragraph directly after the first
//! `# ` heading, or after the `Implements:` line when that is the next non-blank line under the
//! heading. Like the edge lines, a `Settled:` line counts at column 0, outside fenced code and
//! before the first `<!-- loom METADATA` line. Every edit keeps the rest of the text byte for
//! byte, line endings included.

#[cfg(test)]
mod tests;

use std::path::Path;

use super::SettleState;
use crate::context::source_graph::body_hash as sha256;
use crate::fs::knowledge::splice::fence_mask;
use crate::planning::sections::{normalized_hash, sections_of};

const PREFIX: &str = "Settled:";
const METADATA: &str = "<!-- loom METADATA";
const HASH_LEN: usize = 12;

/// The hashes a `Settled:` line records.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettledLine {
    /// The body hash (`body_hash`).
    pub body: String,
    /// The stub hash, for a section or a plan that has a stub.
    pub stub: Option<String>,
    /// The upstream hash, for a plan that implements something.
    pub upstream: Option<String>,
}

/// One line starting `Settled:`, and what it parses to (`None`: the line is malformed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledMark {
    /// Zero-based line.
    pub line: usize,
    /// The parsed line, `None` when it does not parse.
    pub parsed: Option<SettledLine>,
}

/// The first `Settled:` line of `text` that parses.
pub fn settled_line(text: &str) -> Option<SettledLine> {
    settled_marks(text).into_iter().find_map(|mark| mark.parsed)
}

/// Every `Settled:` line of `text`, in order.
pub fn settled_marks(text: &str) -> Vec<SettledMark> {
    let lines = split(text);
    mark_indices(&lines)
        .into_iter()
        .map(|line| SettledMark {
            line,
            parsed: parse(lines[line].content),
        })
        .collect()
}

/// The line, without its line ending.
pub fn render(line: &SettledLine) -> String {
    let mut out = format!("{PREFIX} {}", line.body);
    for (word, hash) in [("stub", &line.stub), ("upstream", &line.upstream)] {
        if let Some(hash) = hash {
            out.push_str(&format!(" {word} {hash}"));
        }
    }
    out
}

/// `text` holding `line`: the first `Settled:` line is replaced in place, else the line is
/// inserted at its place. A text that already holds the same line comes back unchanged.
pub fn mark(text: &str, line: &SettledLine) -> String {
    let lines = split(text);
    let rendered = render(line);
    let Some(&at) = mark_indices(&lines).first() else {
        return insert(text, &lines, &rendered);
    };
    let mut out = String::with_capacity(text.len() + rendered.len());
    for (index, line) in lines.iter().enumerate() {
        out.push_str(if index == at { &rendered } else { line.content });
        out.push_str(line.eol);
    }
    out
}

/// `text` without its `Settled:` lines, each with the blank line `mark` put before it.
pub fn clear(text: &str) -> String {
    let lines = split(text);
    let marks = mark_indices(&lines);
    if marks.is_empty() {
        return text.to_owned();
    }
    let blank = |index: usize| {
        lines
            .get(index)
            .is_some_and(|l| l.content.trim().is_empty())
    };
    let mut dropped = vec![false; lines.len()];
    for at in marks {
        dropped[at] = true;
        if at > 0 && blank(at - 1) {
            dropped[at - 1] = true;
        } else if at == 0 && blank(1) {
            dropped[1] = true;
        }
    }
    // A mark that ended an unterminated text also ended the line above it (see `insert`).
    let unterminated = lines.last().is_some_and(|l| l.eol.is_empty()) && dropped[lines.len() - 1];
    let kept: Vec<&Line> = lines
        .iter()
        .zip(&dropped)
        .filter_map(|(line, dropped)| (!dropped).then_some(line))
        .collect();
    let mut out = String::with_capacity(text.len());
    for (index, line) in kept.iter().enumerate() {
        out.push_str(line.content);
        if !(unterminated && index + 1 == kept.len()) {
            out.push_str(line.eol);
        }
    }
    out
}

/// The body hash: `normalized_hash` of `text` without its `Settled:` line, then for a plan, in
/// path order, each brief's repository path and text.
pub fn body_hash(text: &str, bundle: &[(&Path, &str)]) -> String {
    let own = normalized_hash(&clear(text));
    if bundle.is_empty() {
        return short_hash(&own);
    }
    let mut briefs = bundle.to_vec();
    briefs.sort_by(|a, b| a.0.cmp(b.0));
    let mut parts = vec![own];
    for (path, brief) in briefs {
        parts.push(path.to_string_lossy().into_owned());
        parts.push(normalized_hash(brief));
    }
    combined_hash(&parts)
}

/// The first 12 hex digits of a `normalized_hash` (`sha256:<hex>`).
pub fn short_hash(hash: &str) -> String {
    let hex = hash.strip_prefix("sha256:").unwrap_or(hash);
    hex.chars().take(HASH_LEN).collect()
}

/// The short hash of `parts`, each ended by a newline, in the order given.
pub(crate) fn combined_hash<S: AsRef<str>>(parts: &[S]) -> String {
    let mut joined = String::new();
    for part in parts {
        joined.push_str(part.as_ref());
        joined.push('\n');
    }
    short_hash(&sha256(joined.as_bytes()))
}

/// The settle state of a document holding `recorded` when settling would write `current`.
pub fn state(recorded: Option<&SettledLine>, current: &SettledLine) -> SettleState {
    match recorded {
        None => SettleState::Draft,
        Some(line) if line.body != current.body => SettleState::Revised,
        Some(line) if line.stub != current.stub => SettleState::StubChanged,
        Some(line) if line.upstream != current.upstream => SettleState::UpstreamChanged,
        Some(_) => SettleState::Settled,
    }
}

/// One line of a text: its content and the ending that follows it, `\n`, `\r\n` or `""` (an
/// unterminated last line).
struct Line<'a> {
    content: &'a str,
    eol: &'static str,
}

fn split(text: &str) -> Vec<Line<'_>> {
    text.split_inclusive('\n')
        .map(|piece| match piece.strip_suffix('\n') {
            Some(body) => match body.strip_suffix('\r') {
                Some(content) => Line {
                    content,
                    eol: "\r\n",
                },
                None => Line {
                    content: body,
                    eol: "\n",
                },
            },
            None => Line {
                content: piece,
                eol: "",
            },
        })
        .collect()
}

/// The lines starting `Settled:` at column 0, outside fenced code and before plan metadata.
fn mark_indices(lines: &[Line]) -> Vec<usize> {
    let contents: Vec<&str> = lines.iter().map(|line| line.content).collect();
    let mask = fence_mask(&contents);
    let mut found = Vec::new();
    for (index, content) in contents.iter().enumerate() {
        if mask[index] {
            continue;
        }
        if content.contains(METADATA) {
            break;
        }
        if content.starts_with(PREFIX) {
            found.push(index);
        }
    }
    found
}

/// `Settled: <hash>`, then optionally `stub <hash>` and `upstream <hash>` in that order.
fn parse(content: &str) -> Option<SettledLine> {
    let mut words = content.strip_prefix("Settled: ")?.split_whitespace();
    let mut line = SettledLine {
        body: hash_word(words.next())?,
        ..SettledLine::default()
    };
    let mut next = words.next();
    if next == Some("stub") {
        line.stub = Some(hash_word(words.next())?);
        next = words.next();
    }
    if next == Some("upstream") {
        line.upstream = Some(hash_word(words.next())?);
        next = words.next();
    }
    next.is_none().then_some(line)
}

fn hash_word(word: Option<&str>) -> Option<String> {
    let word = word?;
    let hex = word.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    (word.len() == HASH_LEN && hex).then(|| word.to_owned())
}

/// `text` with `rendered` inserted as its own paragraph at its place: after the first `# `
/// heading (or the `Implements:` line under it), else at the top.
fn insert(text: &str, lines: &[Line], rendered: &str) -> String {
    let Some(at) = placement(text, lines) else {
        let eol = line_ending(lines);
        return format!("{rendered}{eol}{eol}{text}");
    };
    let eol = match lines[at].eol {
        "" => line_ending(lines),
        own => own,
    };
    let mut out = String::with_capacity(text.len() + rendered.len() + 3 * eol.len());
    for (index, line) in lines.iter().enumerate() {
        out.push_str(line.content);
        if index != at {
            out.push_str(line.eol);
            continue;
        }
        out.push_str(eol);
        out.push_str(eol);
        out.push_str(rendered);
        // An anchor that ended an unterminated text leaves the mark unterminated instead.
        out.push_str(line.eol);
    }
    out
}

/// The line the mark goes after: the first level-1 heading, or the `Implements:` line that is
/// the next non-blank line under it.
fn placement(text: &str, lines: &[Line]) -> Option<usize> {
    let headings = sections_of(text).headings;
    let title = headings.iter().find(|h| h.level == 1)?.span.start_line;
    let next = (title + 1..lines.len()).find(|&i| !lines[i].content.trim().is_empty());
    match next {
        Some(index) if lines[index].content.starts_with("Implements:") => Some(index),
        _ => Some(title),
    }
}

/// The text's first line ending, `\n` when it has none.
fn line_ending(lines: &[Line]) -> &'static str {
    lines
        .iter()
        .map(|line| line.eol)
        .find(|eol| !eol.is_empty())
        .unwrap_or("\n")
}
