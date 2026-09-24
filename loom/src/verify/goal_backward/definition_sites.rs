//! Definition sites for plan v2 wiring (DESIGN D11): the line each
//! source-graph node of a file is defined on, found by single-file
//! extraction, so a pattern that only matches the definition of the symbol it
//! names does not pass as wiring.

use std::path::Path;

use crate::context::extract;
use crate::context::source_graph::FileCoverage;

/// `(line, name)` for every node the extractor finds in `path`, where `line`
/// is the 1-indexed line the definition starts on. `None` when the file's
/// definitions cannot be checked: no extractor handles its language, or the
/// extraction degraded (oversized file, parse error, extractor failure) and
/// holds no symbol nodes.
pub(super) fn definition_lines(path: &Path, bytes: &[u8]) -> Option<Vec<(usize, String)>> {
    let extraction = extract::extract_file(&extract::registry(), path, bytes);
    if !matches!(
        extraction.coverage,
        FileCoverage::Full | FileCoverage::Partial { .. }
    ) {
        return None;
    }
    Some(
        extraction
            .nodes
            .into_iter()
            // A symbol's scope ends with its own name; the file node's scope
            // is empty, so it names nothing.
            .filter_map(|node| {
                let name = node.scope.last()?.clone();
                Some((node.span.line_start, name))
            })
            .collect(),
    )
}

#[cfg(test)]
#[path = "definition_sites_tests.rs"]
mod tests;
