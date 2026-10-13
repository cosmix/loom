//! Edge-quality evaluation: a labelled corpus scored against the graph loom
//! builds for it (design 14.2).
//!
//! [`evaluate_dir`] extracts every file of a corpus in memory, resolves the
//! graph with the production resolver, and scores it against the corpus's
//! `labels.yaml`. [`Thresholds::check`] turns a report into violations. Nothing
//! is written and `.loom/` is never touched, so the evaluation runs inside a
//! stage sandbox and in any directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

mod build;
mod ids;
mod labels;
mod metrics;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_build;
#[cfg(test)]
mod tests_floors;
#[cfg(test)]
mod tests_unknown_ids;

use labels::{Expectation, Labels};

pub use labels::LABELS_FILE;

/// The thresholds published before any holdout project was examined, compiled
/// in so an installed binary needs no source tree.
const PUBLISHED_THRESHOLDS: &str = include_str!("../../../eval/edge-quality-thresholds.yaml");

/// How many labels of each kind a corpus carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LabelCounts {
    pub declarations: usize,
    pub targets: usize,
    pub externals: usize,
    pub ambiguous: usize,
    pub impacts: usize,
    /// The impact labels whose depth is 1.
    pub impacts_depth_1: usize,
    pub syntax_error_files: usize,
}

/// Scores of one corpus.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EdgeQualityReport {
    pub dialect: String,
    /// Labelled declarations found, over all labelled declarations.
    pub declaration_recall: f64,
    /// Correct bound targets over bound labelled edges.
    pub target_precision: f64,
    /// Correct bound targets over labels expecting a target.
    pub target_recall: f64,
    /// Labelled references whose edge is unbound with no candidates, or absent.
    pub unresolved_rate: f64,
    /// Labelled references whose edge is unbound with candidates.
    pub ambiguous_rate: f64,
    /// Bound edges at confidence 0.8 or more whose target differs from the label.
    pub false_high_confidence: usize,
    /// Expected ids `impact_with` missed, by labelled depth.
    pub impact_false_negatives: BTreeMap<usize, usize>,
    pub labels: LabelCounts,
    /// Metrics whose denominator was 0; each is a violation.
    pub undefined_ratios: Vec<&'static str>,
    /// One message per labelled id the corpus graph has no node for (a
    /// mistyped label would otherwise score as a miss); each is a violation.
    pub unknown_label_ids: Vec<String>,
    /// One human line per miss.
    pub failures: Vec<String>,
}

/// Score the corpus rooted at `dir`.
pub fn evaluate_dir(dir: &Path) -> Result<EdgeQualityReport> {
    let labels = labels::load_labels(dir)?;
    let graph = build::build_graph(dir, &labels)
        .with_context(|| format!("build the graph of {}", dir.display()))?;
    Ok(metrics::score(&labels, &graph))
}

/// The corpora under `dir`: `dir` itself when it holds a `labels.yaml`,
/// otherwise each child directory that does, in name order. A missing or
/// unreadable `dir` holds none.
pub fn corpus_dirs(dir: &Path) -> Vec<PathBuf> {
    if dir.join(LABELS_FILE).is_file() {
        return vec![dir.to_path_buf()];
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut corpora: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.join(LABELS_FILE).is_file())
        .collect();
    corpora.sort();
    corpora
}

impl LabelCounts {
    pub(crate) fn of(labels: &Labels) -> Self {
        let mut counts = LabelCounts {
            declarations: labels.declarations.len(),
            targets: 0,
            externals: 0,
            ambiguous: 0,
            impacts: labels.impact.len(),
            impacts_depth_1: labels
                .impact
                .iter()
                .filter(|label| label.depth == 1)
                .count(),
            syntax_error_files: labels.syntax_error_files.len(),
        };
        for reference in &labels.references {
            match reference.expect.expectation() {
                Expectation::Target(_) => counts.targets += 1,
                Expectation::External => counts.externals += 1,
                Expectation::Ambiguous(_) => counts.ambiguous += 1,
            }
        }
        counts
    }
}

/// The bars a corpus must clear (design 14.2). The file holds exactly these
/// five keys.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Thresholds {
    pub declaration_recall: f64,
    pub false_high_confidence: usize,
    pub target_precision: f64,
    pub target_recall: f64,
    pub impact_false_negatives_depth_1: usize,
}

/// Read a thresholds file.
pub fn load_thresholds(path: &Path) -> Result<Thresholds> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Thresholds::from_yaml(&text).with_context(|| format!("parse {}", path.display()))
}

impl Thresholds {
    /// Parse a thresholds document.
    pub fn from_yaml(text: &str) -> Result<Self> {
        Ok(crate::yaml::from_str(text)?)
    }

    /// The thresholds compiled into this binary.
    pub fn published() -> Result<Self> {
        Self::from_yaml(PUBLISHED_THRESHOLDS).context("parse the published edge-quality thresholds")
    }

    /// One message per violation; empty when `report` clears every bar.
    ///
    /// A metric with a zero denominator is a violation, so is a labelled id the
    /// corpus graph has no node for, and so is a corpus carrying fewer labels
    /// of any kind than `check_coverage` requires.
    pub fn check(&self, report: &EdgeQualityReport) -> Vec<String> {
        let mut violations: Vec<String> = report
            .undefined_ratios
            .iter()
            .map(|name| format!("{name}: no labels to divide by (0/0 is a failure)"))
            .collect();
        violations.extend(
            report
                .unknown_label_ids
                .iter()
                .map(|message| format!("unknown label id: {message}")),
        );
        self.check_scores(report, &mut violations);
        check_coverage(&report.labels, &mut violations);
        violations
    }

    fn check_scores(&self, report: &EdgeQualityReport, violations: &mut Vec<String>) {
        let floors = [
            (
                "declaration_recall",
                report.declaration_recall,
                self.declaration_recall,
            ),
            (
                "target_precision",
                report.target_precision,
                self.target_precision,
            ),
            ("target_recall", report.target_recall, self.target_recall),
        ];
        for (name, actual, floor) in floors {
            if actual < floor {
                violations.push(format!("{name}: {actual:.3} is below {floor:.3}"));
            }
        }
        if report.false_high_confidence > self.false_high_confidence {
            violations.push(format!(
                "false_high_confidence: {} exceeds {}",
                report.false_high_confidence, self.false_high_confidence
            ));
        }
        match report.impact_false_negatives.get(&1) {
            None => {
                violations.push("impact_false_negatives_depth_1: no depth-1 impact label".into())
            }
            Some(missed) if *missed > self.impact_false_negatives_depth_1 => {
                violations.push(format!(
                    "impact_false_negatives_depth_1: {missed} exceeds {}",
                    self.impact_false_negatives_depth_1
                ));
            }
            Some(_) => {}
        }
    }
}

/// The fewest labels of each kind a corpus may carry, however well it scores:
/// thresholds alone pass a corpus cut down to one label per kind. Each floor is
/// at or just below the smallest count among the corpora under
/// `tests/fixtures/source/labeled`, so a shipped corpus cannot shrink past it
/// without this table changing in review.
const MIN_DECLARATIONS: usize = 10;
const MIN_TARGETS: usize = 6;
const MIN_EXTERNALS: usize = 3;
const MIN_AMBIGUOUS: usize = 1;
const MIN_IMPACTS: usize = 4;
const MIN_IMPACTS_DEPTH_1: usize = 2;
const MIN_SYNTAX_ERROR_FILES: usize = 1;

/// A corpus must carry the floor of every label kind and a syntax-error file.
fn check_coverage(counts: &LabelCounts, violations: &mut Vec<String>) {
    let required = [
        ("declaration labels", counts.declarations, MIN_DECLARATIONS),
        ("target labels", counts.targets, MIN_TARGETS),
        ("external labels", counts.externals, MIN_EXTERNALS),
        ("ambiguous labels", counts.ambiguous, MIN_AMBIGUOUS),
        ("impact labels", counts.impacts, MIN_IMPACTS),
        (
            "depth-1 impact labels",
            counts.impacts_depth_1,
            MIN_IMPACTS_DEPTH_1,
        ),
        (
            "syntax_error_files entries",
            counts.syntax_error_files,
            MIN_SYNTAX_ERROR_FILES,
        ),
    ];
    for (what, count, floor) in required {
        if count < floor {
            violations.push(format!(
                "corpus lacks a full set of {what}: has {count}, needs at least {floor}"
            ));
        }
    }
}
