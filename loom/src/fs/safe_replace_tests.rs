use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use tempfile::TempDir;

use super::{c_name, create_temp, open_parent, publish, replace_no_follow, temp_name, NAME_MAX};

/// A root holding `dir/a.md` with `text`.
fn root_with(text: &str) -> TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(root.path().join("dir")).unwrap();
    std::fs::write(root.path().join("dir/a.md"), text).unwrap();
    root
}

fn read(root: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(root.path().join(rel)).unwrap()
}

/// Every entry of `dir` whose name ends `.tmp`.
fn temp_files(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

#[test]
fn a_normal_replace_writes_the_bytes_and_keeps_the_mode() {
    let root = root_with("old\n");
    let path = root.path().join("dir/a.md");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    replace_no_follow(root.path(), Path::new("dir/a.md"), b"new\n").unwrap();
    assert_eq!(read(&root, "dir/a.md"), "new\n");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o640);
    assert!(temp_files(&root.path().join("dir")).is_empty());
}

#[test]
fn a_symlinked_target_is_refused_and_left_alone() {
    let root = root_with("target\n");
    symlink("a.md", root.path().join("dir/link.md")).unwrap();
    let result = replace_no_follow(root.path(), Path::new("dir/link.md"), b"hijack\n");
    assert!(result.is_err());
    assert_eq!(read(&root, "dir/a.md"), "target\n");
    let link = std::fs::symlink_metadata(root.path().join("dir/link.md")).unwrap();
    assert!(
        link.file_type().is_symlink(),
        "the link itself was replaced"
    );
    assert!(temp_files(&root.path().join("dir")).is_empty());
}

#[test]
fn a_symlinked_directory_component_is_refused() {
    let root = root_with("target\n");
    symlink("dir", root.path().join("linked")).unwrap();
    let result = replace_no_follow(root.path(), Path::new("linked/a.md"), b"hijack\n");
    assert!(result.is_err());
    assert_eq!(read(&root, "dir/a.md"), "target\n");
    assert!(temp_files(&root.path().join("dir")).is_empty());
}

#[test]
fn a_missing_target_is_never_created() {
    let root = root_with("x\n");
    let result = replace_no_follow(root.path(), Path::new("dir/new.md"), b"created\n");
    assert!(result.is_err());
    assert!(!root.path().join("dir/new.md").exists());
    assert!(temp_files(&root.path().join("dir")).is_empty());
}

#[test]
fn a_directory_target_and_a_parent_path_are_refused() {
    let root = root_with("x\n");
    std::fs::create_dir(root.path().join("dir/sub")).unwrap();
    assert!(replace_no_follow(root.path(), Path::new("dir/sub"), b"x").is_err());
    assert!(replace_no_follow(root.path(), Path::new("dir/../dir/a.md"), b"y").is_err());
    assert!(replace_no_follow(root.path(), Path::new("dir/.."), b"y").is_err());
    assert_eq!(read(&root, "dir/a.md"), "x\n");
}

#[test]
fn a_target_name_near_the_limit_still_gets_a_temp_file() {
    // 'é' is two bytes, so the cut at 233 bytes falls inside a character and must step back.
    let names = [
        format!("{}.md", "a".repeat(NAME_MAX - 3)),
        format!("{}.md", "é".repeat(120)),
    ];
    for name in names {
        assert!(name.len() > 233 && name.len() <= NAME_MAX, "{}", name.len());
        let root = root_with("x\n");
        let rel = format!("dir/{name}");
        std::fs::write(root.path().join(&rel), "old\n").unwrap();
        replace_no_follow(root.path(), Path::new(&rel), b"new\n").unwrap();
        assert_eq!(read(&root, &rel), "new\n");
        assert!(temp_files(&root.path().join("dir")).is_empty());
    }
}

#[test]
fn a_temp_name_fits_the_limit_and_stays_valid_utf8() {
    for name in ["a".repeat(NAME_MAX), "é".repeat(127), "x".to_owned()] {
        let tmp = temp_name(name.as_bytes());
        assert!(tmp.len() <= NAME_MAX, "{}", tmp.len());
        assert!(std::str::from_utf8(&tmp).is_ok());
        assert!(tmp.ends_with(b".tmp"));
    }
}

#[test]
fn a_failed_rename_removes_the_temp_file() {
    let root = root_with("x\n");
    let dir = root.path().join("dir");
    std::fs::create_dir(dir.join("sub")).unwrap();
    let parent = open_parent(root.path(), Path::new("dir/a.md")).unwrap();
    let tmp = c_name(&temp_name(b"sub")).unwrap();
    let file = create_temp(&parent, &tmp, 0o644).unwrap();
    assert_eq!(temp_files(&dir).len(), 1);
    // A file cannot be renamed over a directory.
    let target = c_name(b"sub").unwrap();
    assert!(publish(&parent, &tmp, &target, file, 0o644, b"y").is_err());
    assert!(temp_files(&dir).is_empty());
    assert!(dir.join("sub").is_dir());
}
