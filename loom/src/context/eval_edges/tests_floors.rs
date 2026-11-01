//! The label floors: a corpus cut down to a label per kind must not pass.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::labels::load_labels;
use super::{
    check_coverage, corpus_dirs, EdgeQualityReport, LabelCounts, Thresholds, MIN_AMBIGUOUS,
    MIN_DECLARATIONS, MIN_EXTERNALS, MIN_IMPACTS, MIN_IMPACTS_DEPTH_1, MIN_SYNTAX_ERROR_FILES,
    MIN_TARGETS,
};

const AT_THE_FLOORS: LabelCounts = LabelCounts {
    declarations: MIN_DECLARATIONS,
    targets: MIN_TARGETS,
    externals: MIN_EXTERNALS,
    ambiguous: MIN_AMBIGUOUS,
    impacts: MIN_IMPACTS,
    impacts_depth_1: MIN_IMPACTS_DEPTH_1,
    syntax_error_files: MIN_SYNTAX_ERROR_FILES,
};

fn violations_of(counts: &LabelCounts) -> Vec<String> {
    let mut violations = Vec::new();
    check_coverage(counts, &mut violations);
    violations
}

#[test]
fn a_corpus_at_every_floor_passes() {
    assert_eq!(violations_of(&AT_THE_FLOORS), Vec::<String>::new());
}

#[test]
fn a_corpus_one_label_short_of_any_floor_fails_naming_that_kind() {
    type Shrink = fn(&mut LabelCounts);
    let kinds: [(&str, Shrink); 7] = [
        ("declaration labels", |c| c.declarations -= 1),
        ("target labels", |c| c.targets -= 1),
        ("external labels", |c| c.externals -= 1),
        ("ambiguous labels", |c| c.ambiguous -= 1),
        ("impact labels", |c| c.impacts -= 1),
        ("depth-1 impact labels", |c| c.impacts_depth_1 -= 1),
        ("syntax_error_files entries", |c| c.syntax_error_files -= 1),
    ];
    for (kind, drop_one) in kinds {
        let mut counts = AT_THE_FLOORS;
        drop_one(&mut counts);

        let violations = violations_of(&counts);

        assert_eq!(violations.len(), 1, "{kind}: {violations:?}");
        assert!(violations[0].contains(kind), "{kind}: {violations:?}");
    }
}

/// The scenario the floors exist for: every score is perfect and every kind
/// has a label, but a label per kind is all the corpus holds.
#[test]
fn a_one_label_per_kind_corpus_fails_the_published_thresholds() {
    let report = EdgeQualityReport {
        dialect: "rust".to_string(),
        declaration_recall: 1.0,
        target_precision: 1.0,
        target_recall: 1.0,
        unresolved_rate: 0.0,
        ambiguous_rate: 0.0,
        false_high_confidence: 0,
        ambiguous_mismatches: 0,
        impact_false_negatives: BTreeMap::from([(1, 0)]),
        labels: LabelCounts {
            declarations: 1,
            targets: 1,
            externals: 1,
            ambiguous: 1,
            impacts: 1,
            impacts_depth_1: 1,
            syntax_error_files: 1,
        },
        undefined_ratios: Vec::new(),
        unknown_label_ids: Vec::new(),
        failures: Vec::new(),
    };

    let violations = Thresholds::published().expect("thresholds").check(&report);

    for kind in [
        "declaration labels",
        "target labels",
        "external labels",
        "impact labels",
        "depth-1 impact labels",
    ] {
        assert!(
            violations.iter().any(|v| v.contains(kind)),
            "{kind}: {violations:#?}"
        );
    }
}

#[test]
fn label_counts_split_impact_labels_by_depth() {
    let dir = TempDir::new().expect("tempdir");
    fs::write(
        dir.path().join("labels.yaml"),
        "dialect: rust
impact:
  - {start: a, depth: 1, expect: [b]}
  - {start: a, depth: 2, expect: [b]}
  - {start: c, depth: 1, expect: [d]}
",
    )
    .expect("write labels");

    let counts = LabelCounts::of(&load_labels(dir.path()).expect("labels"));

    assert_eq!((counts.impacts, counts.impacts_depth_1), (3, 2));
}

#[test]
fn every_shipped_corpus_meets_the_label_floors() {
    let labeled = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/source/labeled");
    let corpora = corpus_dirs(&labeled);
    assert!(!corpora.is_empty(), "no corpus under {}", labeled.display());

    for corpus in corpora {
        let counts = LabelCounts::of(&load_labels(&corpus).expect("labels"));

        assert_eq!(
            violations_of(&counts),
            Vec::<String>::new(),
            "{}",
            corpus.display()
        );
    }
}
