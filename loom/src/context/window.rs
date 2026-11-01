//! Source windows: the exact lines of a graph node or call site.
//!
//! A window is served only for ids the graph itself names, and only while the
//! file on disk still hashes to the snapshot's `content_hash`. The path that is
//! opened is always a key of `ResolvedGraph::files`, never text the caller
//! supplied, so an id such as `../outside.txt@0-5` cannot reach past the
//! graph.

use std::fmt;
use std::path::Path;

use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{FileCoverage, Span, MAX_EXTRACTED_FILE_BYTES};
use crate::context::untrusted::{flatten_char, inline_safe};
use crate::fs::safe_read::is_not_found;

#[path = "window_stream.rs"]
mod stream;

/// The whole lines a node or site covers, truncated to the caller's line cap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWindow {
    pub path: String,
    /// Byte and line range of `text` within the file.
    pub span: Span,
    pub text: String,
    /// True when `text` stops short of the last line the id covers.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowError {
    /// The id names no graph node and no graph file. Nothing was read.
    UnknownId(String),
    /// The file no longer matches the snapshot (edited or deleted). No bytes
    /// are returned.
    ChangedSinceSnapshot {
        path: String,
    },
    Unreadable {
        path: String,
        detail: String,
    },
}

/// The id comes from the caller and the path from a tracked file name, so
/// neither reaches the terminal with a control character in it.
impl fmt::Display for WindowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WindowError::UnknownId(id) => write!(f, "unknown id: {}", inline_safe(id)),
            WindowError::ChangedSinceSnapshot { path } => {
                write!(f, "changed since snapshot: {}", inline_safe(path))
            }
            WindowError::Unreadable { path, detail } => {
                // The detail ends in the cause, so it is flattened, never cut.
                let detail: String = detail.chars().map(flatten_char).collect();
                write!(f, "cannot read {}: {detail}", inline_safe(path))
            }
        }
    }
}

impl std::error::Error for WindowError {}

/// A byte range inside one graph file.
struct Target {
    path: String,
    start: usize,
    end: usize,
}

/// Return the source of `id`: a node id, or a site id `path@start-end` with
/// byte offsets. The byte range is widened to whole lines and cut to
/// `max_lines` (at least one line).
pub fn read_window(
    graph: &ResolvedGraph,
    project_root: &Path,
    id: &str,
    max_lines: usize,
) -> Result<SourceWindow, WindowError> {
    let target = resolve_target(graph, id).ok_or_else(|| WindowError::UnknownId(id.to_string()))?;
    let entry = graph.files.get(&target.path);
    let expected = entry
        .map(|entry| entry.content_hash.as_str())
        .unwrap_or_default();
    match stream::read(
        project_root,
        &target,
        max_lines.max(1),
        read_limit(entry.map(|entry| &entry.coverage)),
        expected,
    ) {
        Ok(window) => Ok(window),
        Err(stream::ReadError::Changed) => {
            Err(WindowError::ChangedSinceSnapshot { path: target.path })
        }
        Err(stream::ReadError::Io(error)) if is_not_found(&error) => {
            Err(WindowError::ChangedSinceSnapshot { path: target.path })
        }
        Err(stream::ReadError::Io(error)) => Err(WindowError::Unreadable {
            path: target.path,
            detail: format!("{error:#}"),
        }),
    }
}

/// The most bytes a file may hold and still match its snapshot: the size
/// recorded for an oversized file, the extraction cap for any other.
fn read_limit(coverage: Option<&FileCoverage>) -> usize {
    match coverage {
        Some(FileCoverage::Oversized { bytes, .. }) => *bytes,
        _ => MAX_EXTRACTED_FILE_BYTES,
    }
}

/// Map an id onto a graph file and byte range. A node id wins over a site
/// reading of the same text, so a path containing `@` still resolves.
fn resolve_target(graph: &ResolvedGraph, id: &str) -> Option<Target> {
    if let Some(node) = graph.node(id) {
        let path = node.path.to_str()?.to_string();
        return live_file(graph, &path).then_some(Target {
            path,
            start: node.span.start_byte,
            end: node.span.end_byte,
        });
    }
    let (path, range) = id.rsplit_once('@')?;
    let (start, end) = range.split_once('-')?;
    live_file(graph, path).then_some(Target {
        path: path.to_string(),
        start: start.parse().ok()?,
        end: end.parse().ok()?,
    })
}

/// The graph holds `path` and it is not an overlay tombstone.
fn live_file(graph: &ResolvedGraph, path: &str) -> bool {
    graph
        .files
        .get(path)
        .is_some_and(|entry| !matches!(entry.coverage, FileCoverage::Deleted))
}

#[cfg(test)]
#[path = "window_tests.rs"]
mod tests;
