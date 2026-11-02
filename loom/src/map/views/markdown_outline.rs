//! `--outline` for a Markdown file: its ATX headings, read straight from the file.
//!
//! A Markdown file has no extractor in the source graph, so the outline is built from the
//! bytes directly. That needs no graph load and works on untracked files, which keeps it inside
//! the read gate's time bound (`loom-hooks/_read_discipline.sh` parses these rows).
//!
//! Headings are column-zero ATX headings outside fenced code, found with the scanner the
//! knowledge chunker and `replace-section` share. Setext headings (an underlined title) are out
//! of scope, as they are for those two.

use std::path::Path;

use serde_json::{json, Value};

use crate::fs::knowledge::splice::{atx_heading, fence_mask, section_end, strip_closing_sequence};
use crate::fs::safe_read::read_bounded;

use super::json::safe;
use super::matching::MatchMode;
use super::outline::{format_row, outline_header};
use super::project_relative;

/// Largest Markdown file the outline reads.
const MAX_BYTES: usize = 4 * 1024 * 1024;

/// Rows printed before the remainder is summarised, so an outline stays smaller than the file.
const MAX_ROWS: usize = 200;

/// One heading: its 1-based inclusive line range, enclosing headings and own source text.
struct Heading {
    line_start: usize,
    line_end: usize,
    scope: Vec<String>,
    signature: String,
}

/// An outline request: the project-relative path and the headings, or why the file was refused.
struct Outline {
    rel: String,
    headings: Result<Vec<Heading>, String>,
}

/// Whether `arg` names a Markdown file by its extension, ignoring case.
pub fn is_markdown_path(arg: &str) -> bool {
    Path::new(arg)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown"))
}

/// The human outline of the Markdown file `arg`, or the reason it is unavailable.
pub fn render(project_root: &Path, arg: &str) -> String {
    let outline = Outline::load(project_root, arg);
    let mut lines = vec![outline_header(&safe(&outline.rel))];
    match &outline.headings {
        Ok(headings) => {
            lines.extend(headings.iter().take(MAX_ROWS).map(|heading| {
                let range = (heading.line_start, heading.line_end);
                let row = format_row(
                    range,
                    "heading",
                    &heading.scope.join("::"),
                    &heading.signature,
                );
                format!("  {row}")
            }));
            if headings.len() > MAX_ROWS {
                lines.push(format!("... {} more headings", headings.len() - MAX_ROWS));
            }
            lines.push("coverage: markdown headings".to_string());
        }
        Err(reason) => lines.push(format!("coverage: markdown unavailable - {}", safe(reason))),
    }
    lines.join("\n")
}

/// The JSON outline of the Markdown file `arg`, shaped like `outline_json`.
pub fn to_json(project_root: &Path, arg: &str) -> Value {
    let outline = Outline::load(project_root, arg);
    let (coverage, symbols) = match &outline.headings {
        Ok(headings) => (
            json!({"status": "markdown headings"}),
            headings
                .iter()
                .map(|heading| heading_json(&outline.rel, heading))
                .collect(),
        ),
        Err(reason) => (
            json!({"status": "markdown unavailable", "detail": safe(reason)}),
            Vec::new(),
        ),
    };
    json!({
        "path": outline.rel,
        "match": MatchMode::Id.as_str(),
        "coverage": coverage,
        "symbols": symbols,
    })
}

fn heading_json(rel: &str, heading: &Heading) -> Value {
    json!({
        "id": format!("{rel}#L{}", heading.line_start),
        "kind": "heading",
        "scope": heading.scope.iter().map(|segment| safe(segment)).collect::<Vec<_>>(),
        "line_start": heading.line_start,
        "line_end": heading.line_end,
        "signature": safe(&heading.signature),
    })
}

impl Outline {
    fn load(project_root: &Path, arg: &str) -> Self {
        let rel = project_relative(project_root, arg).unwrap_or_else(|| arg.to_string());
        let headings = read_text(project_root, arg, &rel).map(|text| headings_of(&text));
        Self { rel, headings }
    }
}

/// The file's text, or the reason it was refused.
///
/// `project_relative` canonicalizes, which follows a symlink before any read, so the path as
/// given is checked for a symlink first. The bounded read then opens `rel` with no-follow
/// semantics on every component and refuses a path outside the root, an oversized file and
/// anything that is not a regular file.
fn read_text(project_root: &Path, arg: &str, rel: &str) -> Result<String, String> {
    if std::fs::symlink_metadata(arg).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("symbolic link".to_string());
    }
    let bytes = read_bounded(project_root, Path::new(rel), MAX_BYTES)
        .map_err(|error| format!("{error:#}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Every non-empty, non-fenced ATX heading of `text`, in source order.
fn headings_of(text: &str) -> Vec<Heading> {
    let lines: Vec<&str> = text.lines().collect();
    let in_fence = fence_mask(&lines);
    // Titles of the headings enclosing the next one, with their levels.
    let mut open: Vec<(usize, &str)> = Vec::new();
    let mut headings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some((level, raw)) = atx_heading(line).filter(|_| !in_fence[index]) else {
            continue;
        };
        let title = strip_closing_sequence(raw);
        // An empty heading gets no row but still closes the sections it interrupts, as it does
        // for `section_end`, so it must also leave `open`.
        while open.last().is_some_and(|(outer, _)| *outer >= level) {
            open.pop();
        }
        if title.is_empty() {
            continue;
        }
        headings.push(Heading {
            line_start: index + 1,
            line_end: section_end(&lines, &in_fence, index, level),
            scope: open.iter().map(|(_, title)| (*title).to_string()).collect(),
            signature: line.trim().to_string(),
        });
        open.push((level, title));
    }
    headings
}
