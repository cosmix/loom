use crate::fs::knowledge::catalog;
use crate::fs::knowledge::read::{read_knowledge_file, MAX_KNOWLEDGE_FILE_BYTES};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use tempfile::TempDir;

fn write_oversized(root: &Path) {
    fs::write(
        root.join("oversized.md"),
        vec![b'x'; MAX_KNOWLEDGE_FILE_BYTES + 1],
    )
    .unwrap();
}

#[test]
fn a_normal_file_reads() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("notes.md"), "# Notes\n").unwrap();
    let bytes = read_knowledge_file(temp.path(), Path::new("notes.md")).unwrap();
    assert_eq!(bytes, b"# Notes\n");
}

#[test]
fn a_file_over_the_cap_fails_and_the_error_names_it() {
    let temp = TempDir::new().unwrap();
    write_oversized(temp.path());
    let error = read_knowledge_file(temp.path(), Path::new("oversized.md")).unwrap_err();
    assert!(format!("{error:#}").contains("oversized.md"), "{error:#}");
}

#[test]
fn a_symlink_leaf_is_refused() {
    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.md"), "# Secret\n").unwrap();
    symlink(
        outside.path().join("secret.md"),
        temp.path().join("link.md"),
    )
    .unwrap();
    assert!(read_knowledge_file(temp.path(), Path::new("link.md")).is_err());
}

#[test]
fn catalog_build_fails_on_an_oversized_file_and_names_it() {
    let temp = TempDir::new().unwrap();
    write_oversized(temp.path());
    let error = catalog::build(temp.path()).unwrap_err();
    assert!(format!("{error:#}").contains("oversized.md"), "{error:#}");
}

#[test]
fn a_knowledge_root_that_is_a_symlink_still_reads() {
    let temp = TempDir::new().unwrap();
    let real = temp.path().join("real");
    fs::create_dir(&real).unwrap();
    fs::write(real.join("notes.md"), "# Notes\n").unwrap();
    let link = temp.path().join("knowledge");
    symlink(&real, &link).unwrap();
    let bytes = read_knowledge_file(&link, Path::new("notes.md")).unwrap();
    assert_eq!(bytes, b"# Notes\n");
}
