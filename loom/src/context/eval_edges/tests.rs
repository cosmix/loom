//! The evaluator over temp-dir corpora: label loading, metrics, thresholds and
//! the `--eval-edges` exit-code mapping.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::labels::load_labels;
use super::metrics::symbol_matches;
use super::{build, evaluate_dir, load_thresholds, Thresholds};
use crate::map::views::eval_edges;

/// Lines 1-7 are the small corpus the metric tests point at; the rest lifts it
/// to the label floors a corpus must clear.
const MAIN_RS: &str = "fn local() {}
fn run() {
    local();
    parse();
    fetch();
    save();
}
fn step_a() {}
fn step_b() {}
fn step_c() {}
fn idle() {}
fn drive() {
    step_a();
    step_b();
    step_c();
    local();
    missing_one();
    missing_two();
}
";

const PERFECT_LABELS: &str = "dialect: rust
declarations:
  - {path: src/main.rs, kind: function, scope: [local], line: 1}
  - {path: src/main.rs, kind: function, scope: [run], line: 2}
  - {path: src/util.rs, kind: function, scope: [parse], line: 1}
  - {path: src/a.rs, kind: function, scope: [save], line: 1}
  - {path: src/b.rs, kind: function, scope: [save], line: 1}
  - {path: src/main.rs, kind: function, scope: [step_a], line: 8}
  - {path: src/main.rs, kind: function, scope: [step_b], line: 9}
  - {path: src/main.rs, kind: function, scope: [step_c], line: 10}
  - {path: src/main.rs, kind: function, scope: [idle], line: 11}
  - {path: src/main.rs, kind: function, scope: [drive], line: 12}
references:
  - {path: src/main.rs, line: 3, symbol: local, expect: {target: \"src/main.rs#function:local\"}}
  - {path: src/main.rs, line: 4, symbol: parse, expect: {target: \"src/util.rs#function:parse\"}}
  - {path: src/main.rs, line: 5, symbol: fetch, expect: {external: true}}
  - {path: src/main.rs, line: 6, symbol: save, expect: {ambiguous: [\"src/a.rs#function:save\", \"src/b.rs#function:save\"]}}
  - {path: src/main.rs, line: 13, symbol: step_a, expect: {target: \"src/main.rs#function:step_a\"}}
  - {path: src/main.rs, line: 14, symbol: step_b, expect: {target: \"src/main.rs#function:step_b\"}}
  - {path: src/main.rs, line: 15, symbol: step_c, expect: {target: \"src/main.rs#function:step_c\"}}
  - {path: src/main.rs, line: 16, symbol: local, expect: {target: \"src/main.rs#function:local\"}}
  - {path: src/main.rs, line: 17, symbol: missing_one, expect: {external: true}}
  - {path: src/main.rs, line: 18, symbol: missing_two, expect: {external: true}}
impact:
  - {start: \"src/util.rs#function:parse\", depth: 1, expect: [\"src/main.rs#function:run\"]}
  - {start: \"src/main.rs#function:step_a\", depth: 1, expect: [\"src/main.rs#function:drive\"]}
  - {start: \"src/main.rs#function:local\", depth: 2, expect: [\"src/main.rs#function:run\", \"src/main.rs#function:drive\"]}
  - {start: \"src/main.rs#function:step_b\", depth: 3, expect: [\"src/main.rs#function:drive\"]}
syntax_error_files: [src/broken.rs]
";

/// Write `files` under a fresh temp dir.
pub(super) fn corpus(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    write_files(dir.path(), files);
    dir
}

fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let full = root.join(path);
        fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
        fs::write(full, text).expect("write");
    }
}

/// The perfect Rust corpus with `labels` as its `labels.yaml`.
fn perfect_with(labels: &str) -> TempDir {
    corpus(&[
        ("labels.yaml", labels),
        ("src/main.rs", MAIN_RS),
        ("src/util.rs", "pub fn parse() {}\n"),
        ("src/a.rs", "pub fn save() {}\n"),
        ("src/b.rs", "pub fn save() {}\n"),
        (
            "src/broken.rs.txt",
            "pub fn broken() {\n    let value = 1;\n",
        ),
    ])
}

pub(super) fn rust_labels(references: &str) -> String {
    format!("dialect: rust\ndeclarations: []\nreferences:\n{references}impact: []\n")
}

#[test]
fn the_label_loader_rejects_unknown_keys_and_malformed_expectations() {
    let cases = [
        ("dialect: rust\nbogus: 1\n", "a top-level unknown key"),
        (
            "dialect: rust\nreferences:\n  - {path: a.rs, line: 1, symbol: f, extra: 1, expect: {external: true}}\n",
            "an unknown reference key",
        ),
        (
            "dialect: rust\nreferences:\n  - {path: a.rs, line: 1, symbol: f, expect: {external: true, target: x}}\n",
            "two expectations",
        ),
        (
            "dialect: rust\nreferences:\n  - {path: a.rs, line: 1, symbol: f, expect: {}}\n",
            "no expectation",
        ),
        (
            "dialect: rust\nreferences:\n  - {path: a.rs, line: 1, symbol: f, expect: {external: false}}\n",
            "external: false",
        ),
        ("dialect: cobol\n", "an unknown dialect"),
    ];
    for (labels, what) in cases {
        let dir = corpus(&[("labels.yaml", labels)]);
        assert!(load_labels(dir.path()).is_err(), "{what} must be rejected");
    }
}

#[test]
fn a_perfect_tiny_corpus_scores_one_and_clears_the_published_thresholds() {
    let dir = perfect_with(PERFECT_LABELS);

    let report = evaluate_dir(dir.path()).expect("evaluate");

    assert_eq!(report.dialect, "rust");
    assert_eq!(report.declaration_recall, 1.0, "{report:#?}");
    assert_eq!(report.target_precision, 1.0, "{report:#?}");
    assert_eq!(report.target_recall, 1.0, "{report:#?}");
    assert_eq!(report.false_high_confidence, 0, "{report:#?}");
    assert_eq!(
        report.impact_false_negatives.get(&1),
        Some(&0),
        "{report:#?}"
    );
    assert!(report.failures.is_empty(), "{report:#?}");
    assert!(report.undefined_ratios.is_empty(), "{report:#?}");
    let published = Thresholds::published().expect("published thresholds");
    assert_eq!(published.check(&report), Vec::<String>::new());
}

#[test]
fn a_wrong_high_confidence_bind_counts_in_false_high_confidence() {
    let dir = corpus(&[
        (
            "labels.yaml",
            rust_labels(
                "  - {path: src/lib.rs, line: 4, symbol: helper, expect: {target: \"src/lib.rs#function:other\"}}\n",
            )
            .as_str(),
        ),
        (
            "src/lib.rs",
            "fn helper() {}\nfn other() {}\nfn run() {\n    helper();\n}\n",
        ),
    ]);

    let report = evaluate_dir(dir.path()).expect("evaluate");

    assert_eq!(report.false_high_confidence, 1, "{report:#?}");
    assert_eq!(report.target_precision, 0.0, "{report:#?}");
    assert_eq!(report.failures.len(), 1, "{report:#?}");
    assert!(
        report.failures[0].contains("src/lib.rs:4 helper"),
        "{report:#?}"
    );
}

#[test]
fn an_ambiguous_label_is_correct_only_when_the_candidates_match() {
    let wrong = PERFECT_LABELS.replace(
        "[\"src/a.rs#function:save\", \"src/b.rs#function:save\"]",
        "[\"src/a.rs#function:save\"]",
    );
    assert_ne!(wrong, PERFECT_LABELS);

    let matching = evaluate_dir(perfect_with(PERFECT_LABELS).path()).expect("evaluate");
    let mismatched = evaluate_dir(perfect_with(&wrong).path()).expect("evaluate");

    assert!(matching.failures.is_empty(), "{matching:#?}");
    assert_eq!(mismatched.failures.len(), 1, "{mismatched:#?}");
    assert_eq!(mismatched.ambiguous_mismatches, 1);
    for json in [false, true] {
        let outcome = eval_edges::run(perfect_with(&wrong).path(), None, json).expect("run");
        assert_eq!(outcome.exit_code, 1, "{}", outcome.text);
        assert!(
            outcome.text.contains("ambiguous_mismatches"),
            "{}",
            outcome.text
        );
    }
    assert!(
        mismatched.failures[0].contains("src/main.rs:6 save"),
        "{mismatched:#?}"
    );
}

#[test]
fn a_txt_syntax_error_file_loads_as_its_real_extension() {
    let dir = perfect_with(PERFECT_LABELS);
    let labels = load_labels(dir.path()).expect("labels");

    let graph = build::build_graph(dir.path(), &labels).expect("graph");

    assert!(
        graph.files.contains_key("src/broken.rs"),
        "{:?}",
        graph.files.keys()
    );
    assert!(!graph.files.contains_key("src/broken.rs.txt"));
    assert!(!graph.files.contains_key("labels.yaml"));
}

#[test]
fn a_listed_syntax_error_file_that_is_not_stored_is_an_error() {
    let dir = corpus(&[
        (
            "labels.yaml",
            "dialect: rust\nsyntax_error_files: [src/missing.rs]\n",
        ),
        ("src/lib.rs", "fn a() {}\n"),
    ]);

    let error = evaluate_dir(dir.path()).expect_err("a missing syntax-error file");

    assert!(format!("{error:#}").contains("src/missing.rs"), "{error:#}");
}

#[test]
fn a_label_symbol_matches_a_qualified_edge_but_not_a_longer_name() {
    assert!(symbol_matches("parse", "parse"));
    assert!(symbol_matches("util::parse", "parse"));
    assert!(symbol_matches("obj.parse", "parse"));
    assert!(symbol_matches("Widget::new", "Widget::new"));
    assert!(!symbol_matches("reparse", "parse"));
    assert!(!symbol_matches("util::reparse", "parse"));
    assert!(!symbol_matches("parse", "util::parse"));
}

#[test]
fn a_corpus_with_no_labels_has_failing_zero_ratios() {
    let dir = corpus(&[
        (
            "labels.yaml",
            "dialect: rust\ndeclarations: []\nreferences: []\nimpact: []\n",
        ),
        ("src/lib.rs", "fn a() {}\n"),
    ]);

    let report = evaluate_dir(dir.path()).expect("evaluate");
    let violations = Thresholds::published().expect("thresholds").check(&report);

    for name in ["declaration_recall", "target_precision", "target_recall"] {
        assert!(report.undefined_ratios.contains(&name), "{report:#?}");
        assert!(
            violations.iter().any(|v| v.starts_with(name)),
            "{name}: {violations:#?}"
        );
    }
    assert!(
        violations.iter().any(|v| v.contains("corpus lacks a")),
        "{violations:#?}"
    );
    assert!(
        violations.iter().any(|v| v.contains("depth-1 impact")),
        "{violations:#?}"
    );
}

#[test]
fn thresholds_reject_unknown_keys_and_load_from_a_file() {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("t.yaml");
    fs::write(&path, "declaration_recall: 1.0\nextra: 1\n").expect("write");
    assert!(load_thresholds(&path).is_err());

    fs::write(
        &path,
        "declaration_recall: 0.5\nfalse_high_confidence: 2\ntarget_precision: 0.1\n\
         target_recall: 0.1\nimpact_false_negatives_depth_1: 3\n",
    )
    .expect("write");
    let loaded = load_thresholds(&path).expect("load");
    assert_eq!(loaded.false_high_confidence, 2);
    assert_eq!(loaded.impact_false_negatives_depth_1, 3);
}

#[test]
fn no_labelled_corpus_exits_two_and_prints_the_summary() {
    let dir = TempDir::new().expect("tempdir");

    let outcome = eval_edges::run(dir.path(), None, false).expect("run");

    assert_eq!(outcome.exit_code, 2);
    assert_eq!(outcome.summary, "0 corpora, 0 failing");
    assert_eq!(outcome.text.lines().last(), Some("0 corpora, 0 failing"));
}

#[test]
fn a_failing_threshold_exits_one_and_a_clean_corpus_exits_zero() {
    let thin = corpus(&[
        (
            "labels.yaml",
            "dialect: rust\ndeclarations: []\nreferences: []\nimpact: []\n",
        ),
        ("src/lib.rs", "fn a() {}\n"),
    ]);
    let failing = eval_edges::run(thin.path(), None, false).expect("run");
    assert_eq!(failing.exit_code, 1, "{}", failing.text);
    assert_eq!(failing.summary, "1 corpora, 1 failing");
    assert_eq!(failing.text.lines().last(), Some("1 corpora, 1 failing"));

    let clean = perfect_with(PERFECT_LABELS);
    let passing = eval_edges::run(clean.path(), None, false).expect("run");
    assert_eq!(passing.exit_code, 0, "{}", passing.text);
    assert_eq!(passing.summary, "1 corpora, 0 failing");
}

#[test]
fn a_directory_of_corpora_scores_each_child_and_renders_json() {
    let parent = TempDir::new().expect("tempdir");
    let clean = perfect_with(PERFECT_LABELS);
    copy_tree(clean.path(), &parent.path().join("rust"));
    write_files(
        &parent.path().join("thin"),
        &[
            (
                "labels.yaml",
                "dialect: rust\ndeclarations: []\nreferences: []\nimpact: []\n",
            ),
            ("src/lib.rs", "fn a() {}\n"),
        ],
    );

    let outcome = eval_edges::run(parent.path(), None, true).expect("run");

    assert_eq!(outcome.exit_code, 1);
    assert_eq!(outcome.summary, "2 corpora, 1 failing");
    let value: serde_json::Value = serde_json::from_str(&outcome.text).expect("json");
    assert_eq!(value["corpora"], 2);
    assert_eq!(value["failing"], 1);
    assert_eq!(value["summary"], "2 corpora, 1 failing");
    assert_eq!(value["results"][0]["name"], "rust");
    assert_eq!(value["results"][0]["pass"], true);
    assert_eq!(value["results"][1]["pass"], false);
}

#[test]
fn a_thresholds_file_overrides_the_published_bars() {
    let thin = corpus(&[
        (
            "labels.yaml",
            "dialect: rust\ndeclarations: []\nreferences: []\nimpact: []\n",
        ),
        ("src/lib.rs", "fn a() {}\n"),
    ]);
    let bars = TempDir::new().expect("tempdir");
    let path = bars.path().join("bars.yaml");
    fs::write(
        &path,
        "declaration_recall: 0.0\nfalse_high_confidence: 0\ntarget_precision: 0.0\n\
         target_recall: 0.0\nimpact_false_negatives_depth_1: 0\n",
    )
    .expect("write");

    let outcome = eval_edges::run(thin.path(), Some(&path), false).expect("run");

    // The zero-denominator and missing-label violations survive zero bars.
    assert_eq!(outcome.exit_code, 1, "{}", outcome.text);
}

/// Copy every file under `from` into `to`, keeping relative paths.
fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("read_dir") {
        let path = entry.expect("entry").path();
        let target = to.join(path.file_name().expect("name"));
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::create_dir_all(to).expect("mkdir");
            fs::copy(&path, &target).expect("copy");
        }
    }
}
