//! Design 14.2's metrics: labels scored against the graph loom built.

use anyhow::{anyhow, bail, Result};
use std::collections::{BTreeMap, BTreeSet};

use super::ids::unknown_label_ids;
use super::labels::{Expectation, Labels, ReferenceLabel};
use super::{EdgeQualityReport, LabelCounts};
use crate::context::graph_store::ResolvedGraph;
use crate::context::resolve::{impact_with, ImpactOptions};
use crate::context::source_graph::{SourceEdge, SourceEdgeKind};

/// A bound edge at or above this confidence asserts its target.
const HIGH_CONFIDENCE: f32 = 0.8;

/// The edge kinds a reference label can name.
const SEMANTIC_KINDS: [SourceEdgeKind; 4] = [
    SourceEdgeKind::Calls,
    SourceEdgeKind::References,
    SourceEdgeKind::Implements,
    SourceEdgeKind::Extends,
];

/// Score `labels` against `graph`.
pub(super) fn score(labels: &Labels, graph: &ResolvedGraph) -> Result<EdgeQualityReport> {
    let mut failures = Vec::new();
    let mut undefined_ratios = Vec::new();
    let matched = count_declarations(labels, graph, &mut failures)?;
    let tally = tally_references(labels, graph, &mut failures)?;
    let counts = LabelCounts::of(labels);
    let references = labels.references.len();
    let mut rate = |n, d, name| ratio(n, d, name, &mut undefined_ratios);
    let declaration_recall = rate(matched, labels.declarations.len(), "declaration_recall");
    let target_precision = rate(tally.correct_targets, tally.bound, "target_precision");
    let target_recall = rate(tally.correct_targets, counts.targets, "target_recall");
    let unresolved_rate = rate(tally.unresolved, references, "unresolved_rate");
    let ambiguous_rate = rate(tally.ambiguous, references, "ambiguous_rate");
    let impact_false_negatives = count_impact_misses(labels, graph, &mut failures);
    Ok(EdgeQualityReport {
        dialect: labels.dialect.clone(),
        declaration_recall,
        target_precision,
        target_recall,
        unresolved_rate,
        ambiguous_rate,
        false_high_confidence: tally.false_high_confidence,
        ambiguous_mismatches: tally.ambiguous_mismatches,
        impact_false_negatives,
        labels: counts,
        undefined_ratios,
        unknown_label_ids: unknown_label_ids(labels, graph),
        failures,
    })
}

/// `numerator / denominator`; a zero denominator records `name` as undefined
/// and yields 0.0, which `Thresholds::check` reports as a violation.
fn ratio(
    numerator: usize,
    denominator: usize,
    name: &'static str,
    undefined: &mut Vec<&'static str>,
) -> f64 {
    if denominator == 0 {
        undefined.push(name);
        return 0.0;
    }
    numerator as f64 / denominator as f64
}

/// Match each declaration's labelled line to a node's source span.
fn count_declarations(
    labels: &Labels,
    graph: &ResolvedGraph,
    failures: &mut Vec<String>,
) -> Result<usize> {
    let mut matched = 0;
    let mut sites = BTreeSet::new();
    for label in &labels.declarations {
        let found = graph.files.get(&label.path).and_then(|entry| {
            entry.nodes.iter().find(|node| {
                node.kind.as_str() == label.kind
                    && node.scope == label.scope
                    && node.span.line_start <= label.line
                    && label.line <= node.span.line_end
            })
        });
        if let Some(node) = found {
            if !sites.insert((
                &label.path,
                &label.kind,
                &label.scope,
                node.span.start_byte,
                node.span.end_byte,
            )) {
                bail!(
                    "{}:{} declaration {} duplicates a declaration site",
                    label.path,
                    label.line,
                    label.scope.join("::")
                );
            }
            matched += 1;
        } else {
            failures.push(format!(
                "{}:{} declaration {} {}: expected a node, found none",
                label.path,
                label.line,
                label.kind,
                label.scope.join("::")
            ));
        }
    }
    Ok(matched)
}

#[derive(Default)]
struct ReferenceTally {
    bound: usize,
    correct_targets: usize,
    unresolved: usize,
    ambiguous: usize,
    false_high_confidence: usize,
    ambiguous_mismatches: usize,
}

fn tally_references(
    labels: &Labels,
    graph: &ResolvedGraph,
    failures: &mut Vec<String>,
) -> Result<ReferenceTally> {
    let mut tally = ReferenceTally::default();
    let mut sites = BTreeSet::new();
    for label in &labels.references {
        let expectation = label.expect.expectation();
        let observation = find_edge(graph, label)?;
        if let Some((_, start, end)) = observation {
            if !sites.insert((&label.path, start, end)) {
                bail!(
                    "{}:{} {} duplicates a reference site",
                    label.path,
                    label.line,
                    label.symbol
                );
            }
        }
        let edge = observation.map(|(edge, _, _)| edge);
        let is_bound = edge.is_some_and(|edge| !edge.is_unresolved());
        match edge {
            Some(edge) if is_bound => {
                tally.bound += 1;
                let differs = !matches!(&expectation, Expectation::Target(t) if edge.to == *t);
                if differs && edge.confidence >= HIGH_CONFIDENCE {
                    tally.false_high_confidence += 1;
                }
            }
            Some(edge) if !edge.candidates.is_empty() => tally.ambiguous += 1,
            _ => tally.unresolved += 1,
        }
        if is_correct(&expectation, edge) {
            if matches!(expectation, Expectation::Target(_)) {
                tally.correct_targets += 1;
            }
        } else {
            if matches!(expectation, Expectation::Ambiguous(_)) {
                tally.ambiguous_mismatches += 1;
            }
            failures.push(describe_miss(label, &expectation, edge));
        }
    }
    Ok(tally)
}

/// Whether `edge` (a missing edge counts as unbound) satisfies `expectation`.
fn is_correct(expectation: &Expectation<'_>, edge: Option<&SourceEdge>) -> bool {
    let unbound = edge.is_none_or(SourceEdge::is_unresolved);
    match expectation {
        Expectation::Target(target) => edge.is_some_and(|edge| edge.to == *target),
        Expectation::External => unbound,
        Expectation::Ambiguous(expected) => {
            unbound
                && edge.is_some_and(|edge| {
                    edge.candidates.iter().collect::<BTreeSet<_>>()
                        == expected.iter().collect::<BTreeSet<_>>()
                })
        }
    }
}

/// The edge a label names: from its path, at its line, for its symbol.
/// Multiple sites require an explicit byte offset, independent of expectation.
fn find_edge<'a>(
    graph: &'a ResolvedGraph,
    label: &ReferenceLabel,
) -> Result<Option<(&'a SourceEdge, usize, usize)>> {
    let Some(entry) = graph.files.get(&label.path) else {
        if label.site_start.is_some() {
            return Err(missing_reference_site(label));
        }
        return Ok(None);
    };
    let matches: Vec<(&SourceEdge, usize, usize)> = entry
        .edges
        .iter()
        .filter(|edge| {
            SEMANTIC_KINDS.contains(&edge.kind) && symbol_matches(&edge.symbol, &label.symbol)
        })
        .flat_map(|edge| {
            edge.sites
                .iter()
                .filter(move |site| {
                    site.line_start <= label.line
                        && label.line <= site.line_end
                        && label
                            .site_start
                            .is_none_or(|start| start == site.start_byte)
                })
                .map(move |site| (edge, site.start_byte, site.end_byte))
        })
        .collect();
    if matches.len() > 1 {
        bail!(
            "{}:{} {} matches multiple reference sites; specify site_start",
            label.path,
            label.line,
            label.symbol
        );
    }
    if matches.is_empty() && label.site_start.is_some() {
        return Err(missing_reference_site(label));
    }
    Ok(matches.first().copied())
}

fn missing_reference_site(label: &ReferenceLabel) -> anyhow::Error {
    anyhow!(
        "{}:{} {} has no reference site at site_start",
        label.path,
        label.line,
        label.symbol
    )
}

/// `written` equals `label`, or ends with it after a `::` or `.` separator.
pub(super) fn symbol_matches(written: &str, label: &str) -> bool {
    written
        .strip_suffix(label)
        .is_some_and(|head| head.is_empty() || head.ends_with("::") || head.ends_with('.'))
}

fn describe_miss(
    label: &ReferenceLabel,
    expectation: &Expectation<'_>,
    edge: Option<&SourceEdge>,
) -> String {
    let expected = match expectation {
        Expectation::Target(target) => format!("target {target}"),
        Expectation::External => "unbound (external)".to_string(),
        Expectation::Ambiguous(ids) => format!("unbound with candidates [{}]", ids.join(", ")),
    };
    let actual = match edge {
        None => "no edge".to_string(),
        Some(edge) if edge.is_unresolved() => {
            format!("unbound with candidates [{}]", edge.candidates.join(", "))
        }
        Some(edge) => format!(
            "bound to {} ({} {:.2})",
            edge.to, edge.provenance, edge.confidence
        ),
    };
    format!(
        "{}:{} {}: expected {expected}; actual {actual}",
        label.path, label.line, label.symbol
    )
}

/// Expected ids missing from `impact_with(start, depth d)`, summed per depth.
/// Every labelled depth has an entry, zero when nothing is missing.
fn count_impact_misses(
    labels: &Labels,
    graph: &ResolvedGraph,
    failures: &mut Vec<String>,
) -> BTreeMap<usize, usize> {
    let mut misses = BTreeMap::new();
    for label in &labels.impact {
        let options = ImpactOptions {
            max_depth: label.depth,
            ..ImpactOptions::default()
        };
        let reached: BTreeSet<String> = impact_with(graph, &label.start, &options)
            .hits
            .into_iter()
            .map(|hit| hit.id)
            .collect();
        let count = misses.entry(label.depth).or_insert(0);
        for id in label.expect.iter().filter(|id| !reached.contains(*id)) {
            *count += 1;
            failures.push(format!(
                "impact {} depth {}: expected {id}; actual not reached",
                label.start, label.depth
            ));
        }
    }
    misses
}
