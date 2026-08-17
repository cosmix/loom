//! Honest coverage reporting over a [`ResolvedGraph`].
//!
//! The source graph never hides a degraded file: an unsupported language, an
//! oversized file, and a parse error all keep their file-level node and their
//! [`FileCoverage`](crate::context::source_graph::FileCoverage) tag. This
//! module turns that per-file honesty into a single summary that preserves
//! it — every status and every edge provenance is counted and shown, never
//! filtered down to "the good parts".

use std::collections::BTreeMap;
use std::fmt;

use crate::context::graph_store::ResolvedGraph;

/// A summary of a [`ResolvedGraph`]'s coverage.
///
/// Counts every file the graph knows about, including files that failed to
/// parse or exceeded the size cap — those are reported, never dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoverageReport {
    /// Files present in the graph.
    pub files: usize,
    /// File count per `FileCoverage::status()` value, e.g. "full" -> 388.
    pub files_by_status: BTreeMap<&'static str, usize>,
    /// Files whose coverage reports symbol-level extraction (`FileCoverage::has_symbols`).
    pub symbol_level_files: usize,
    pub nodes: usize,
    pub edges: usize,
    /// Edge count per `EdgeProvenance::as_str()` value, e.g. "parser" -> 6100.
    pub edges_by_provenance: BTreeMap<&'static str, usize>,
    /// Edges still pointing at `UNRESOLVED_TARGET`.
    pub unresolved_edges: usize,
    /// Revision the underlying base layer describes; empty when there is none
    /// (an overlay-only view before any base was ever published).
    pub base_revision: String,
    /// Files this view's overlay shadowed over the base — non-zero means the
    /// report describes a stage-local view, not the base alone.
    pub overlaid_files: usize,
}

impl CoverageReport {
    /// Summarise a resolved graph. Counts every file, including degraded ones.
    pub fn of(graph: &ResolvedGraph) -> Self {
        let mut files_by_status: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut symbol_level_files = 0usize;

        for entry in graph.files.values() {
            *files_by_status.entry(entry.coverage.status()).or_insert(0) += 1;
            if entry.coverage.has_symbols() {
                symbol_level_files += 1;
            }
        }

        let mut edges_by_provenance: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut unresolved_edges = 0usize;

        for edge in graph.edges() {
            *edges_by_provenance
                .entry(edge.provenance.as_str())
                .or_insert(0) += 1;
            if edge.is_unresolved() {
                unresolved_edges += 1;
            }
        }

        CoverageReport {
            files: graph.files.len(),
            files_by_status,
            symbol_level_files,
            nodes: graph.node_count(),
            edges: graph.edge_count(),
            edges_by_provenance,
            unresolved_edges,
            base_revision: graph.base_revision.clone(),
            overlaid_files: graph.overlaid.len(),
        }
    }

    /// Fraction of files that got symbol-level extraction, in `0.0..=1.0`.
    /// Returns 0.0 for an empty graph - never divides by zero.
    pub fn symbol_level_fraction(&self) -> f32 {
        if self.files == 0 {
            return 0.0;
        }
        self.symbol_level_files as f32 / self.files as f32
    }

    /// Render [`Self::symbol_level_fraction`] as a whole-number percentage,
    /// except a non-zero fraction that would otherwise round down to `0%`
    /// renders as `<1%` — "0%" reads as "none had symbols", which is false.
    fn symbol_level_percent(&self) -> String {
        let fraction = self.symbol_level_fraction();
        let percent = (fraction * 100.0).round();
        if fraction > 0.0 && percent < 1.0 {
            "<1%".to_string()
        } else {
            format!("{percent}%")
        }
    }

    /// First 8 characters of [`Self::base_revision`], or `"none"` when empty.
    fn short_base_revision(&self) -> &str {
        if self.base_revision.is_empty() {
            "none"
        } else {
            self.base_revision.get(..8).unwrap_or(&self.base_revision)
        }
    }
}

/// Render a `key count` list from a status/provenance breakdown map, or an
/// empty string when the map is empty - callers decide whether to wrap the
/// result in a parenthetical group.
fn render_breakdown(counts: &BTreeMap<&'static str, usize>) -> String {
    counts
        .iter()
        .map(|(name, count)| format!("{name} {count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl fmt::Display for CoverageReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base_suffix = format!(
            " - base {}, {} overlaid",
            self.short_base_revision(),
            self.overlaid_files
        );

        if self.files == 0 {
            return write!(f, "coverage: 0 files - no files indexed{base_suffix}");
        }

        let status_group = render_breakdown(&self.files_by_status);
        let status_group = if status_group.is_empty() {
            String::new()
        } else {
            format!(" ({status_group})")
        };

        let edges_group = render_breakdown(&self.edges_by_provenance);
        let edges_group = if edges_group.is_empty() {
            String::new()
        } else {
            format!(" ({edges_group}; {} unresolved)", self.unresolved_edges)
        };

        write!(
            f,
            "coverage: {} files{status_group} - {} symbol-level - {} nodes, {} edges{edges_group}{base_suffix}",
            self.files,
            self.symbol_level_percent(),
            self.nodes,
            self.edges
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::graph_store::FileEntry;
    use crate::context::source_graph::{
        FileCoverage, NodeLanguage, SourceEdge, SourceEdgeKind, SourceNode, SourceNodeKind, Span,
    };
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn node(id: &str, path: &str) -> SourceNode {
        SourceNode {
            id: id.to_string(),
            kind: SourceNodeKind::File,
            path: PathBuf::from(path),
            scope: Vec::new(),
            span: Span::default(),
            signature: String::new(),
            body_hash: "sha256:deadbeef".to_string(),
            language: NodeLanguage::Rust,
            parser_version: "test+v1".to_string(),
            coverage: FileCoverage::Full,
        }
    }

    fn entry(coverage: FileCoverage, nodes: Vec<SourceNode>, edges: Vec<SourceEdge>) -> FileEntry {
        FileEntry {
            content_hash: "sha256:abc".to_string(),
            nodes,
            edges,
            coverage,
        }
    }

    fn graph(files: Vec<(&str, FileEntry)>) -> ResolvedGraph {
        let mut map = BTreeMap::new();
        for (path, entry) in files {
            map.insert(path.to_string(), entry);
        }
        ResolvedGraph {
            base_revision: "rev1".to_string(),
            overlaid: Default::default(),
            files: map,
        }
    }

    #[test]
    fn counts_are_right_for_a_mixed_graph() {
        let full_edge = SourceEdge::parser("a#file:", "b#file:", SourceEdgeKind::Calls, "f");
        let unresolved = SourceEdge::unresolved("a#file:", SourceEdgeKind::Calls, "mystery");

        let g = graph(vec![
            (
                "src/a.rs",
                entry(
                    FileCoverage::Full,
                    vec![node("src/a.rs", "src/a.rs")],
                    vec![full_edge, unresolved],
                ),
            ),
            (
                "src/b.rs",
                entry(
                    FileCoverage::Full,
                    vec![node("src/b.rs", "src/b.rs")],
                    vec![],
                ),
            ),
            (
                "vendor/lib.min.js",
                entry(
                    FileCoverage::LexicalOnly {
                        detail: "no extractor".to_string(),
                    },
                    vec![node("vendor/lib.min.js", "vendor/lib.min.js")],
                    vec![],
                ),
            ),
        ]);

        let report = CoverageReport::of(&g);

        assert_eq!(report.files, 3);
        assert_eq!(report.files_by_status.get("full"), Some(&2));
        assert_eq!(report.files_by_status.get("lexical-only"), Some(&1));
        assert_eq!(report.symbol_level_files, 2);
        assert_eq!(report.nodes, 3);
        assert_eq!(report.edges, 2);
        assert_eq!(report.edges_by_provenance.get("parser"), Some(&1));
        assert_eq!(report.edges_by_provenance.get("inferred"), Some(&1));
        assert_eq!(report.unresolved_edges, 1);
        assert_eq!(report.base_revision, "rev1");
        assert_eq!(report.overlaid_files, 0);
    }

    #[test]
    fn a_parse_error_file_is_reported_not_hidden() {
        // Regression: a graph containing a degraded file must still surface
        // it in `files_by_status` rather than silently dropping it.
        let g = graph(vec![(
            "src/broken.rs",
            entry(
                FileCoverage::ParseError {
                    span: Span::default(),
                    detail: "unexpected token".to_string(),
                },
                vec![node("src/broken.rs", "src/broken.rs")],
                vec![],
            ),
        )]);

        let report = CoverageReport::of(&g);

        assert_eq!(report.files, 1);
        assert_eq!(report.files_by_status.get("parse-error"), Some(&1));
        assert_eq!(report.symbol_level_files, 0);

        let rendered = report.to_string();
        assert!(
            rendered.contains("parse-error 1"),
            "display output missing parse-error group: {rendered}"
        );
    }

    #[test]
    fn symbol_level_fraction_on_an_empty_graph_is_zero_and_does_not_panic() {
        let report = CoverageReport::default();
        assert_eq!(report.symbol_level_fraction(), 0.0);
        assert_eq!(
            report.to_string(),
            "coverage: 0 files - no files indexed - base none, 0 overlaid"
        );
    }

    #[test]
    fn display_renders_the_documented_one_line_shape() {
        let g = graph(vec![
            (
                "src/a.rs",
                entry(
                    FileCoverage::Full,
                    vec![node("src/a.rs", "src/a.rs")],
                    vec![],
                ),
            ),
            (
                "src/b.rs",
                entry(
                    FileCoverage::LexicalOnly {
                        detail: "unsupported".to_string(),
                    },
                    vec![node("src/b.rs", "src/b.rs")],
                    vec![SourceEdge::inferred(
                        "src/b.rs#file:",
                        "src/a.rs#file:",
                        SourceEdgeKind::References,
                        "thing",
                        0.4,
                    )],
                ),
            ),
        ]);

        let report = CoverageReport::of(&g);
        let rendered = report.to_string();

        assert!(rendered.starts_with("coverage: 2 files ("));
        assert!(rendered.contains("full 1"));
        assert!(rendered.contains("lexical-only 1"));
        assert!(rendered.contains("50% symbol-level"));
        assert!(rendered.contains("2 nodes, 1 edges (inferred 1; 0 unresolved)"));
        assert!(rendered.ends_with("- base rev1, 0 overlaid"));
        assert!(!rendered.ends_with('\n'));
    }

    #[test]
    fn base_revision_and_overlaid_files_are_reported_and_the_revision_is_truncated() {
        let mut files = BTreeMap::new();
        files.insert(
            "src/a.rs".to_string(),
            entry(
                FileCoverage::Full,
                vec![node("src/a.rs", "src/a.rs")],
                vec![],
            ),
        );
        let mut overlaid = BTreeSet::new();
        overlaid.insert("src/a.rs".to_string());

        let g = ResolvedGraph {
            base_revision: "0123456789abcdef".to_string(),
            overlaid,
            files,
        };

        let report = CoverageReport::of(&g);
        assert_eq!(report.base_revision, "0123456789abcdef");
        assert_eq!(report.overlaid_files, 1);
        assert!(report.to_string().ends_with("- base 01234567, 1 overlaid"));
    }

    #[test]
    fn symbol_level_percent_renders_sub_one_percent_as_lt_one() {
        let mut files = BTreeMap::new();
        files.insert(
            "src/a.rs".to_string(),
            entry(
                FileCoverage::Full,
                vec![node("src/a.rs", "src/a.rs")],
                vec![],
            ),
        );
        for i in 0..999 {
            files.insert(
                format!("src/gen_{i}.rs"),
                entry(
                    FileCoverage::LexicalOnly {
                        detail: "unsupported".to_string(),
                    },
                    vec![node(&format!("src/gen_{i}.rs"), &format!("src/gen_{i}.rs"))],
                    vec![],
                ),
            );
        }
        let g = ResolvedGraph {
            base_revision: String::new(),
            overlaid: Default::default(),
            files,
        };

        let report = CoverageReport::of(&g);
        assert_eq!(report.files, 1000);
        assert_eq!(report.symbol_level_files, 1);
        assert!(report.to_string().contains("<1% symbol-level"));
    }
}
