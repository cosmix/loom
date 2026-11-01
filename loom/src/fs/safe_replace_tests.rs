use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;

use tempfile::TempDir;

use super::{
    c_name, create_temp, open_parent, publish, publish_in_dir, replace_no_follow, replace_private,
    restore_no_follow, temp_name, FAIL_DIR_SYNC, NAME_MAX,
};

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
fn restore_replaces_a_hard_link_without_changing_its_other_name() {
    let root = root_with("original");
    let original = root.path().join("dir/a.md");
    let target = root.path().join("dir/target.md");
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::hard_link(&original, &target).unwrap();
    restore_no_follow(root.path(), Path::new("dir/target.md"), b"restored", 0o644).unwrap();
    assert_eq!(std::fs::read(&original).unwrap(), b"original");
    assert_eq!(
        std::fs::metadata(&original).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"restored");
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o644
    );
    restore_no_follow(root.path(), Path::new("dir/new.md"), b"new", 0o644).unwrap();
    assert_eq!(read(&root, "dir/new.md"), "new");
}

#[test]
fn opened_parent_stays_anchored_when_its_path_is_replaced() {
    let root = root_with("original");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("a.md"), "outside").unwrap();
    let parent = open_parent(root.path(), Path::new("dir/a.md")).unwrap();
    std::fs::rename(root.path().join("dir"), root.path().join("moved")).unwrap();
    symlink(outside.path(), root.path().join("dir")).unwrap();

    publish_in_dir(&parent, std::ffi::OsStr::new("a.md"), b"new", 0o600, true).unwrap();

    assert_eq!(read(&root, "moved/a.md"), "new");
    assert_eq!(
        std::fs::read_to_string(outside.path().join("a.md")).unwrap(),
        "outside"
    );
}

#[test]
fn sync_failure_after_rename_still_reports_publication() {
    let root = root_with("original");
    FAIL_DIR_SYNC.with(|flag| flag.set(true));
    let result = replace_no_follow(root.path(), Path::new("dir/a.md"), b"new");
    FAIL_DIR_SYNC.with(|flag| flag.set(false));
    result.unwrap();
    assert_eq!(read(&root, "dir/a.md"), "new");
}

#[test]
fn reporting_publisher_distinguishes_sync_warning_from_failure() {
    let root = root_with("original");
    let parent = open_parent(root.path(), Path::new("dir/a.md")).unwrap();
    FAIL_DIR_SYNC.with(|flag| flag.set(true));
    let result = super::publish_in_dir_reporting(
        &parent,
        std::ffi::OsStr::new("a.md"),
        b"new",
        0o666,
        false,
    );
    FAIL_DIR_SYNC.with(|flag| flag.set(false));
    assert!(result.unwrap().is_some());
    assert_eq!(read(&root, "dir/a.md"), "new");
    assert!(super::publish_in_dir_reporting(
        &parent,
        std::ffi::OsStr::new("bad\0name"),
        b"x",
        0o666,
        false
    )
    .is_err());
    assert_eq!(read(&root, "dir/a.md"), "new");
}

#[test]
fn private_replacement_creates_missing_file_and_refuses_symlinked_parent() {
    let root = root_with("original");
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), root.path().join("link")).unwrap();
    assert!(replace_private(root.path(), Path::new("link/escape"), b"x").is_err());
    assert!(!outside.path().join("escape").exists());
    replace_private(root.path(), Path::new("dir/new"), b"created").unwrap();
    assert_eq!(read(&root, "dir/new"), "created");
    assert_eq!(
        std::fs::metadata(root.path().join("dir/new"))
            .unwrap()
            .permissions()
            .mode()
            & 0o077,
        0
    );
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
    assert!(publish(&parent, &tmp, &target, file, 0o644, true, b"y").is_err());
    assert!(temp_files(&dir).is_empty());
    assert!(dir.join("sub").is_dir());
}
