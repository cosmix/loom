use super::*;

#[test]
fn rule_18_reports_only_indexes_over_the_byte_limit() {
    let oversized = TempDir::new().unwrap();
    fs::write(
        oversized.path().join("INDEX.md"),
        "x".repeat(OVER_INDEX_BUDGET),
    )
    .unwrap();
    assert_eq!(
        build(oversized.path()).unwrap().issues,
        vec![CatalogIssue::OversizedIndex {
            bytes: OVER_INDEX_BUDGET as u64
        }]
    );

    let small = TempDir::new().unwrap();
    fs::write(small.path().join("INDEX.md"), "small index").unwrap();
    assert!(!build(small.path())
        .unwrap()
        .issues
        .iter()
        .any(|issue| matches!(issue, CatalogIssue::OversizedIndex { .. })));
}

/// Write an `INDEX.md` of exactly `bytes` bytes and assert its size.
fn index_of_exactly(root: &Path, bytes: usize) {
    let path = root.join("INDEX.md");
    fs::write(&path, "x".repeat(bytes)).unwrap();
    assert_eq!(fs::metadata(&path).unwrap().len(), bytes as u64);
}

#[test]
fn rule_19_reports_index_only_strictly_over_the_byte_boundary() {
    let at = TempDir::new().unwrap();
    index_of_exactly(at.path(), size::MAX_INDEX_BYTES as usize);
    assert!(build(at.path()).unwrap().issues.is_empty());

    let over = TempDir::new().unwrap();
    index_of_exactly(over.path(), OVER_INDEX_BUDGET);
    assert_eq!(
        build(over.path()).unwrap().issues,
        vec![CatalogIssue::OversizedIndex {
            bytes: OVER_INDEX_BUDGET as u64
        }]
    );
}

#[test]
fn an_index_of_twenty_thousand_bytes_is_within_budget() {
    let temp = TempDir::new().unwrap();
    index_of_exactly(temp.path(), 20_000);
    assert!(build(temp.path()).unwrap().issues.is_empty());
}

#[test]
fn an_index_over_the_new_budget_is_reported() {
    let temp = TempDir::new().unwrap();
    index_of_exactly(temp.path(), 24_577);
    assert_eq!(
        build(temp.path()).unwrap().issues,
        vec![CatalogIssue::OversizedIndex { bytes: 24_577 }]
    );
}

/// The fingerprint skips a symlink at `INDEX.md`, so the byte budget must not
/// measure the file it points at.
#[cfg(unix)]
#[test]
fn a_symlinked_index_is_not_measured_against_the_byte_budget() {
    let temp = TempDir::new().unwrap();
    let target = temp.path().join("big-index.txt");
    fs::write(&target, "x".repeat(OVER_INDEX_BUDGET)).unwrap();
    let root = temp.path().join("knowledge");
    fs::create_dir(&root).unwrap();
    std::os::unix::fs::symlink(&target, root.join("INDEX.md")).unwrap();

    assert!(!build(&root)
        .unwrap()
        .issues
        .iter()
        .any(|issue| matches!(issue, CatalogIssue::OversizedIndex { .. })));
}

#[test]
fn a_knowledge_root_that_is_a_regular_file_has_no_fingerprints() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("knowledge");
    fs::write(&root, "not a directory").unwrap();

    assert!(curated_tree_fingerprints(&root).unwrap().is_empty());
}
