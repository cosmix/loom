//! Portfolio census: what the source graph can and cannot see in one or more
//! checkouts.
//!
//! Every tracked file is classified (excluded, vendored, generated, eligible,
//! unsupported), grouped by subproject, and measured by files and by bytes.
//! Symbol-level coverage of the eligible files comes from the resolved graph
//! when the root is the checkout `loom map` runs in, and from an in-memory
//! extraction otherwise; a census never writes under any root it inspects.

use anyhow::{bail, Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::context::extract::dialect::DialectSpec;
use crate::context::extract::{extract_file, extractor_for, registry, BoxedExtractor, Lookup};
use crate::context::graph_store::ResolvedGraph;
use crate::context::refresh::excluded;
use crate::context::source_graph::{FileCoverage, MAX_EXTRACTED_FILE_BYTES};
use crate::fs::safe_read::read_bounded;
use crate::git::runner::run_git_pinned_checked;

mod classify;
mod report;
mod subprojects;

pub(crate) use classify::{attributes, attributes_at_head};
use classify::{Class, Classified};
pub use report::{
    CensusReport, ClassTotals, Count, CoverageSource, DialectCoverage, ParseStats, RootCensus,
    SubprojectCensus, CENSUS_SCHEMA,
};
use subprojects::SubprojectIndex;

#[derive(Debug, Clone, Default)]
pub struct CensusOptions {
    /// Checkouts to census. Empty means the current project.
    pub roots: Vec<PathBuf>,
}

impl CensusOptions {
    /// Whether `project`, the checkout `loom map` runs in, is one of the
    /// censused roots. Only then does the census read its resolved graph.
    pub fn covers(&self, project: &Path) -> bool {
        self.roots.is_empty() || self.roots.iter().any(|root| same_directory(project, root))
    }
}

/// Census every root in `options`. `current` is the project root and resolved
/// graph of the checkout `loom map` runs in; a root equal to it is measured
/// from that graph, any other root (or a current root whose graph holds no
/// files) by in-memory extraction.
pub fn run(
    options: &CensusOptions,
    current: Option<(&Path, &ResolvedGraph)>,
) -> Result<CensusReport> {
    let roots: Vec<&Path> = match (options.roots.is_empty(), current) {
        (false, _) => options.roots.iter().map(PathBuf::as_path).collect(),
        (true, Some((project, _))) => vec![project],
        (true, None) => bail!("no census root: pass a git work tree"),
    };
    let extractors = registry();
    let mut report = CensusReport {
        roots: Vec::with_capacity(roots.len()),
        totals: ClassTotals::default(),
    };
    for root in roots {
        let graph = current
            .filter(|(project, _)| same_directory(project, root))
            .map(|(_, graph)| graph)
            // A graph with no files (never built, or unavailable) sees
            // nothing; reading coverage from it would report every eligible
            // file as lexical-only.
            .filter(|graph| !graph.files.is_empty());
        let census = census_root(root, graph, &extractors)
            .with_context(|| format!("census of {} failed", root.display()))?;
        report.totals.merge(&census.totals);
        report.roots.push(census);
    }
    Ok(report)
}

fn same_directory(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn census_root(
    root: &Path,
    graph: Option<&ResolvedGraph>,
    extractors: &[BoxedExtractor],
) -> Result<RootCensus> {
    let inside = run_git_pinned_checked(&["rev-parse", "--is-inside-work-tree"], root)
        .with_context(|| format!("{} is not a git work tree", root.display()))?;
    if inside != "true" {
        bail!("{} is not a git work tree", root.display());
    }
    let tracked = classify::tracked_files(root)?;
    let candidates: Vec<&str> = tracked
        .iter()
        .filter(|entry| entry.non_utf8.is_none())
        .map(|entry| entry.path.as_str())
        .filter(|path| !excluded(path))
        .collect();
    let attributes = classify::attributes(root, &candidates)?;
    // Per-file probes refuse a symlink at every component, the root included.
    let physical = root
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", root.display()))?;

    let mut tally = Tally {
        index: SubprojectIndex::new(tracked.iter().map(|entry| entry.path.as_str())),
        coverage: Coverage::new(&physical, graph, extractors),
        extractors,
        totals: ClassTotals::default(),
        subprojects: BTreeMap::new(),
    };
    for entry in &tracked {
        if let Some(classified) = classify::classify(&physical, entry, &attributes) {
            tally.add(&entry.path, classified);
        }
    }
    Ok(RootCensus {
        root: root.display().to_string(),
        coverage_source: if graph.is_some() {
            CoverageSource::Graph
        } else {
            CoverageSource::InMemory
        },
        parse: tally.coverage.into_stats(),
        totals: tally.totals,
        subprojects: tally.subprojects.into_values().collect(),
    })
}

/// The running counts of one root.
struct Tally<'a> {
    index: SubprojectIndex,
    coverage: Coverage<'a>,
    extractors: &'a [BoxedExtractor],
    totals: ClassTotals,
    subprojects: BTreeMap<String, SubprojectCensus>,
}

impl Tally<'_> {
    fn add(&mut self, path: &str, classified: Classified) {
        let subproject = self
            .subprojects
            .entry(self.index.assign(path).to_string())
            .or_insert_with_key(|dir| SubprojectCensus {
                path: dir.clone(),
                totals: ClassTotals::default(),
                by_dialect: BTreeMap::new(),
                unsupported_by_extension: BTreeMap::new(),
            });
        slot(&mut self.totals, classified.class).add_file(classified.bytes);
        slot(&mut subproject.totals, classified.class).add_file(classified.bytes);
        match classified.class {
            Class::Eligible(dialect) => {
                let outcome = self.coverage.outcome(path, classified.bytes);
                record_dialect(
                    subproject,
                    dialect,
                    extractor_status(self.extractors, path),
                    classified.bytes,
                    outcome,
                );
            }
            Class::Unsupported => subproject
                .unsupported_by_extension
                .entry(classified.extension)
                .or_default()
                .add_file(classified.bytes),
            Class::Excluded | Class::Vendored | Class::Generated => {}
        }
    }
}

/// The class's counter inside `totals`.
fn slot(totals: &mut ClassTotals, class: Class) -> &mut Count {
    match class {
        Class::Excluded => &mut totals.excluded,
        Class::Vendored => &mut totals.vendored,
        Class::Generated => &mut totals.generated,
        Class::Eligible(_) => &mut totals.eligible,
        Class::Unsupported => &mut totals.unsupported,
    }
}

fn record_dialect(
    subproject: &mut SubprojectCensus,
    dialect: &'static DialectSpec,
    extractor: String,
    bytes: u64,
    outcome: Outcome,
) {
    let entry = subproject
        .by_dialect
        .entry(dialect.id.to_string())
        .or_insert_with(|| DialectCoverage {
            extractor,
            ..DialectCoverage::default()
        });
    entry.files += 1;
    entry.bytes = entry.bytes.saturating_add(bytes);
    match outcome {
        Outcome::Symbols => {
            entry.symbol_files += 1;
            entry.symbol_bytes = entry.symbol_bytes.saturating_add(bytes);
        }
        Outcome::ParseError => entry.parse_errors += 1,
        Outcome::Lexical => {}
    }
}

fn extractor_status(extractors: &[BoxedExtractor], path: &str) -> String {
    match extractor_for(extractors, Path::new(path)) {
        Lookup::Extractor(_) => "available".to_string(),
        Lookup::Gap { detail, .. } => format!("gap: {detail}"),
        Lookup::Unknown => "none".to_string(),
    }
}

/// How an eligible file fared at extraction.
#[derive(Clone, Copy)]
enum Outcome {
    /// Symbols were extracted (`full` or `partial`).
    Symbols,
    ParseError,
    /// File-level only: a gap, an oversized or unreadable file, or a file the
    /// graph does not hold.
    Lexical,
}

impl Outcome {
    fn of(coverage: &FileCoverage) -> Self {
        match coverage {
            FileCoverage::Full | FileCoverage::Partial { .. } => Outcome::Symbols,
            FileCoverage::ParseError { .. } => Outcome::ParseError,
            _ => Outcome::Lexical,
        }
    }
}

/// Where a root's per-file coverage comes from.
struct Coverage<'a> {
    root: &'a Path,
    graph: Option<&'a ResolvedGraph>,
    extractors: &'a [BoxedExtractor],
    stats: ParseStats,
    spent: Duration,
}

impl<'a> Coverage<'a> {
    fn new(
        root: &'a Path,
        graph: Option<&'a ResolvedGraph>,
        extractors: &'a [BoxedExtractor],
    ) -> Self {
        Self {
            root,
            graph,
            extractors,
            stats: ParseStats::default(),
            spent: Duration::ZERO,
        }
    }

    fn outcome(&mut self, path: &str, size: u64) -> Outcome {
        if let Some(graph) = self.graph {
            return graph
                .files
                .get(path)
                .map_or(Outcome::Lexical, |entry| Outcome::of(&entry.coverage));
        }
        // The graph builder's Oversized cap: such a file is never read.
        if size > MAX_EXTRACTED_FILE_BYTES as u64 {
            return Outcome::Lexical;
        }
        let started = Instant::now();
        let outcome = match read_bounded(self.root, Path::new(path), MAX_EXTRACTED_FILE_BYTES) {
            Ok(bytes) => {
                let extraction = extract_file(self.extractors, Path::new(path), &bytes);
                self.stats.files_parsed += 1;
                self.stats.bytes_read = self.stats.bytes_read.saturating_add(bytes.len() as u64);
                Outcome::of(&extraction.coverage)
            }
            Err(_) => Outcome::Lexical,
        };
        self.spent += started.elapsed();
        outcome
    }

    /// Parse statistics for an in-memory root; `None` when a graph served it.
    fn into_stats(mut self) -> Option<ParseStats> {
        self.stats.elapsed_ms = u64::try_from(self.spent.as_millis()).unwrap_or(u64::MAX);
        self.graph.is_none().then_some(self.stats)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_hardening;
