use crate::fs::knowledge::catalog::*;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn rule_1_walks_markdown_recursively_and_sorts_while_skipping_generated_files() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("zdir")).unwrap();
    fs::create_dir_all(temp.path().join(".hidden")).unwrap();
    fs::write(temp.path().join("b.md"), "## B\n").unwrap();
    fs::write(temp.path().join("INDEX.md"), "## Generated\n").unwrap();
    fs::write(temp.path().join("zdir/a.md"), "## A\n").unwrap();
    fs::write(temp.path().join(".hidden/ignored.md"), "## Hidden\n").unwrap();
    let catalog = build(temp.path()).unwrap();
    assert_eq!(
        catalog
            .chunks
            .iter()
            .map(|chunk| chunk.file.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("b.md"), PathBuf::from("zdir/a.md")]
    );
}

#[test]
fn rule_2_passes_relative_forward_slash_paths_to_chunks() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("architecture")).unwrap();
    fs::write(temp.path().join("architecture/topic.md"), "## Topic\n").unwrap();
    let catalog = build(temp.path()).unwrap();
    assert_eq!(
        catalog.chunks[0].file,
        PathBuf::from("architecture/topic.md")
    );
    assert!(!catalog.chunks[0].file.is_absolute());
}

#[test]
fn rule_3_hashes_sorted_chunk_identity_lines_for_revision() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("z.md"), "## Z\n").unwrap();
    fs::write(temp.path().join("a.md"), "## A\n").unwrap();
    let catalog = build(temp.path()).unwrap();
    let mut lines: Vec<_> = catalog
        .chunks
        .iter()
        .map(|chunk| format!("{}:{}\n", chunk.id, chunk.content_hash))
        .collect();
    lines.sort();
    assert_eq!(
        catalog.revision,
        hex::encode(Sha256::digest(lines.concat().as_bytes()))
    );
}

#[test]
fn rule_4_is_deterministic_for_identical_bytes() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes.md"), "## Title\n`src/none.rs`\n").unwrap();
    let first = build(temp.path()).unwrap();
    let second = build(temp.path()).unwrap();
    assert_eq!(first.revision, second.revision);
    assert_eq!(
        serde_json::to_vec(&first).unwrap(),
        serde_json::to_vec(&second).unwrap()
    );
}

#[test]
fn rule_5_reports_duplicate_normalized_headings_per_file() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes.md"), "## Locking!\n## locking\n").unwrap();
    let catalog = build(temp.path()).unwrap();
    assert_eq!(
        catalog.issues,
        vec![CatalogIssue::DuplicateHeading {
            file: PathBuf::from("notes.md"),
            heading: "locking".to_string(),
            occurrences: 2,
        }]
    );
}

#[test]
fn rule_6_reports_scaffold_generic_blurbs() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join("architecture")).unwrap();
    let blurb = "Topic notes for the architecture knowledge area.";
    fs::write(
        temp.path().join("architecture/topic.md"),
        format!("> {blurb}\n## Topic\n"),
    )
    .unwrap();
    let catalog = build(temp.path()).unwrap();
    assert_eq!(
        catalog.issues,
        vec![CatalogIssue::GenericBlurb {
            file: PathBuf::from("architecture/topic.md"),
            blurb: blurb.to_string(),
        }]
    );
}

#[test]
fn rule_7_reports_broken_markdown_links() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("notes.md"),
        "## Topic\n[Missing](missing.md)\n",
    )
    .unwrap();
    let catalog = build(temp.path()).unwrap();
    assert_eq!(
        catalog.issues,
        vec![CatalogIssue::BrokenLink {
            file: PathBuf::from("notes.md"),
            target: "missing.md".to_string(),
        }]
    );
}

#[test]
fn rule_8_reports_missing_repository_source_paths_when_project_root_is_known() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().join("project/doc/loom/knowledge");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("notes.md"), "## Topic\n`src/missing.rs`\n").unwrap();
    let catalog = build(&root).unwrap();
    assert_eq!(
        catalog.issues,
        vec![CatalogIssue::MissingSourceRef {
            file: PathBuf::from("notes.md"),
            source_path: "src/missing.rs".to_string(),
        }]
    );
}

#[test]
fn rule_9_sorts_issues_by_file_kind_and_payload() {
    let temp = TempDir::new().unwrap();
    fs::write(
        temp.path().join("a.md"),
        "## Repeat\n## repeat\n[Missing](missing.md)\n",
    )
    .unwrap();
    fs::write(temp.path().join("b.md"), "## Topic\n[Missing](other.md)\n").unwrap();
    let catalog = build(temp.path()).unwrap();
    assert!(matches!(
        catalog.issues.first(),
        Some(CatalogIssue::DuplicateHeading { .. })
    ));
    assert!(matches!(
        catalog.issues.get(1),
        Some(CatalogIssue::BrokenLink { file, .. }) if file == &PathBuf::from("a.md")
    ));
    assert!(matches!(
        catalog.issues.get(2),
        Some(CatalogIssue::BrokenLink { file, .. }) if file == &PathBuf::from("b.md")
    ));
}

#[test]
fn rule_10_returns_empty_catalog_for_missing_or_empty_root() {
    let temp = TempDir::new().unwrap();
    let empty = build(temp.path()).unwrap();
    let missing = build(&temp.path().join("missing")).unwrap();
    let empty_hash = hex::encode(Sha256::digest(b""));
    assert_eq!(empty.revision, empty_hash);
    assert!(empty.chunks.is_empty() && empty.issues.is_empty());
    assert_eq!(missing, empty);
}
