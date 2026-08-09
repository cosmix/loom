use super::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_detect_rust_project() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("Cargo.toml"), "[package]").unwrap();

    let result = detect_project_type(temp.path()).unwrap();
    assert!(result.contains("Rust"));
}

#[test]
fn test_detect_node_project() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("package.json"), "{}").unwrap();

    let result = detect_project_type(temp.path()).unwrap();
    assert!(result.contains("Node.js"));
}

#[test]
fn test_find_entry_points() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("main.rs"), "fn main() {}").unwrap();

    let result = find_entry_points(temp.path(), None).unwrap();
    assert!(result.contains("main.rs"));
}

#[test]
fn test_count_todos() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), "// TODO: fix this\n// TODO: and this").unwrap();

    let count = count_pattern_in_source(temp.path(), "TODO").unwrap();
    assert_eq!(count, 2);
}

#[cfg(unix)]
#[test]
fn source_scan_does_not_follow_directory_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.rs"), "// TODO: external").unwrap();
    symlink(outside.path(), temp.path().join("outside-link")).unwrap();

    let count = count_pattern_in_source(temp.path(), "TODO").unwrap();
    assert_eq!(count, 0);
}

#[cfg(unix)]
#[test]
fn source_scan_does_not_recurse_through_symlink_cycles() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("lib.rs"), "// TODO: local").unwrap();
    symlink(temp.path(), src.join("loop")).unwrap();

    let count = count_pattern_in_source(temp.path(), "TODO").unwrap();
    assert_eq!(count, 1);
}
