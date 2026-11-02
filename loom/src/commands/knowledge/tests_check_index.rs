//! `loom knowledge check` reports the stale `INDEX.md` the landing gate refuses.

use super::*;
use crate::fs::knowledge::catalog::render_baseline;
use tempfile::TempDir;

fn knowledge_root(temp: &TempDir) -> PathBuf {
    let root = temp.path().join("doc/loom/knowledge");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("patterns.md"),
        "# Patterns\n\n> Patterns of this project.\n\n## One\n\nText.\n",
    )
    .unwrap();
    root
}

fn has_stale_index(catalog: &Catalog) -> bool {
    catalog
        .issues
        .iter()
        .any(|issue| matches!(issue, CatalogIssue::StaleIndex))
}

#[test]
fn a_stale_index_is_a_strict_issue_that_names_the_fix() {
    let temp = TempDir::new().unwrap();
    let root = knowledge_root(&temp);
    index::write_index(&root).unwrap();
    std::fs::write(
        root.join("patterns.md"),
        "# Patterns\n\n> Patterns of this project.\n\n## One\n\nText.\n\nMore.\n",
    )
    .unwrap();

    let catalog = diagnose(&root).unwrap();

    assert!(has_stale_index(&catalog), "{:?}", catalog.issues);
    assert_eq!(strict_issue_count(&catalog.issues), 1);
    let report = human_report(&root, &catalog);
    assert!(report.contains("INDEX.md is stale"), "{report}");
    assert!(report.contains("loom knowledge sync"), "{report}");
}

#[test]
fn a_synced_index_is_not_reported() {
    let temp = TempDir::new().unwrap();
    let root = knowledge_root(&temp);
    index::write_index(&root).unwrap();

    assert!(!has_stale_index(&diagnose(&root).unwrap()));
}

#[test]
fn a_missing_index_is_not_reported() {
    let temp = TempDir::new().unwrap();
    let root = knowledge_root(&temp);

    assert!(!has_stale_index(&diagnose(&root).unwrap()));
}

/// A recorded baseline must not let a landing's refusal pass `--strict`.
#[test]
fn a_written_baseline_never_records_a_stale_index() {
    let temp = TempDir::new().unwrap();
    let root = knowledge_root(&temp);
    std::fs::write(root.join(INDEX_FILENAME), "stale\n").unwrap();

    let recorded = render_baseline(&catalog::build(&root).unwrap().issues);
    let baseline = CheckBaseline::parse(&recorded).unwrap();

    let catalog = diagnose(&root).unwrap();
    let comparison = baseline.compare(&catalog.issues);
    assert_eq!(comparison.new.len(), 1, "{:?}", comparison.new);
    assert!(matches!(comparison.new[0], CatalogIssue::StaleIndex));
}
