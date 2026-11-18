use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{
    c_name, create_temp, is_publish_temp, open_parent, publish, publish_in_dir, replace_no_follow,
    replace_private, restore_no_follow, temp_name, BEFORE_CANONICALIZE, FAIL_DIR_SYNC, NAME_MAX,
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
        assert!(is_publish_temp(&tmp), "{}", String::from_utf8_lossy(&tmp));
    }
}

#[test]
fn only_the_publish_temp_shape_is_recognized() {
    assert!(is_publish_temp(b".a.json.0123456789abcdef.tmp"));
    for name in [
        &b"a.json"[..],
        b"..0123456789abcdef.tmp",
        b".a.json.0123456789ABCDEF.tmp",
        b".a.json.0123456789abcde.tmp",
        b".a.json-0123456789abcdef.tmp",
        b"a.json.0123456789abcdef.tmp",
        b".a.json.0123456789abcdef",
        b".tmp",
    ] {
        assert!(!is_publish_temp(name), "{}", String::from_utf8_lossy(name));
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

/// `base/real/proj/dir/a.md` and `base/alias` -> `base/real`, as macOS's `/var` link spells
/// a temp dir two ways.
fn linked_project() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let base = tempfile::tempdir().expect("tempdir");
    let real = base.path().canonicalize().unwrap().join("real/proj");
    std::fs::create_dir_all(real.join("dir")).unwrap();
    std::fs::write(real.join("dir/a.md"), "old\n").unwrap();
    let alias = base.path().join("alias");
    symlink(base.path().join("real"), &alias).unwrap();
    (base, real, alias.join("proj"))
}

#[test]
fn a_root_spelled_through_a_link_above_the_tree_is_matched_either_way() {
    let (_base, real, aliased) = linked_project();
    for (target, root) in [(&aliased, &real), (&real, &aliased)] {
        let file = target.join("dir/a.md");
        crate::fs::safe_fs::safe_replace_outside_workdir(&file, root, b"new\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(real.join("dir/a.md")).unwrap(),
            "new\n"
        );
        std::fs::write(real.join("dir/a.md"), "old\n").unwrap();
    }
}

#[test]
fn a_link_inside_the_tree_is_still_refused_when_the_root_is_spelled_differently() {
    let (_base, real, aliased) = linked_project();
    symlink(real.join("dir"), real.join("link")).unwrap();
    let file = aliased.join("link/a.md");
    assert!(crate::fs::safe_fs::safe_replace_outside_workdir(&file, &real, b"new\n").is_err());
    assert_eq!(
        std::fs::read_to_string(real.join("dir/a.md")).unwrap(),
        "old\n"
    );
    let outside = real.parent().unwrap().join("other.md");
    std::fs::write(&outside, "old\n").unwrap();
    let err = crate::fs::safe_fs::safe_replace_outside_workdir(&outside, &aliased, b"new\n");
    assert!(format!("{:#}", err.unwrap_err()).contains("not under project_root"));
}

#[test]
fn the_cut_is_made_at_the_shortest_ancestor_so_an_in_tree_link_is_walked() {
    let (_base, real, aliased) = linked_project();
    symlink(&real, real.join("up")).unwrap();
    let file = aliased.join("up/dir/a.md");
    assert!(crate::fs::safe_fs::safe_replace_outside_workdir(&file, &real, b"new\n").is_err());
    assert_eq!(
        std::fs::read_to_string(real.join("dir/a.md")).unwrap(),
        "old\n"
    );
}

#[test]
fn a_root_that_is_itself_a_link_is_refused_however_the_target_is_spelled() {
    let (base, real, _aliased) = linked_project();
    let link = base.path().join("rootlink");
    symlink(&real, &link).unwrap();
    // The lexical match reaches safe_open_dirfd, the other spelling the slow path's lstat; a
    // trailing separator must not make that lstat follow the link.
    let slashed = PathBuf::from(format!("{}/", link.display()));
    for (target, root, refusal) in [
        (&link, &link, "Failed to open dirfd"),
        (&slashed, &slashed, "Failed to open dirfd"),
        (&real, &link, "is a symlink"),
        (&real, &slashed, "is a symlink"),
    ] {
        let file = target.join("dir/a.md");
        let err = crate::fs::safe_fs::safe_replace_outside_workdir(&file, root, b"new\n");
        // The top-level message names the refusal's cause.
        let message = err.unwrap_err().to_string();
        assert!(message.contains(refusal), "{message}");
    }
    assert_eq!(
        std::fs::read_to_string(real.join("dir/a.md")).unwrap(),
        "old\n"
    );
}

/// Installs the `BEFORE_CANONICALIZE` hook and clears it on drop, so a test that fails before the
/// hook runs cannot leave it set for a later test on the same thread.
struct BeforeCanonicalize;

impl BeforeCanonicalize {
    fn set(hook: impl FnOnce() + 'static) -> Self {
        BEFORE_CANONICALIZE.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
        Self
    }
}

impl Drop for BeforeCanonicalize {
    fn drop(&mut self) {
        BEFORE_CANONICALIZE.with(|slot| slot.borrow_mut().take());
    }
}

#[test]
fn a_root_swapped_for_a_link_while_it_is_resolved_is_refused() {
    let (base, real, aliased) = linked_project();
    let decoy = base.path().join("decoy");
    std::fs::create_dir_all(decoy.join("dir")).unwrap();
    std::fs::write(decoy.join("dir/a.md"), "old\n").unwrap();
    let (moved, swapped) = (base.path().join("moved"), real.clone());
    let decoy_target = decoy.clone();
    let _hook = BeforeCanonicalize::set(move || {
        std::fs::rename(&swapped, &moved).unwrap();
        symlink(&decoy_target, &swapped).unwrap();
    });
    let err = crate::fs::safe_fs::safe_replace_outside_workdir(
        &aliased.join("dir/a.md"),
        &real,
        b"new\n",
    );
    let message = err.unwrap_err().to_string();
    assert!(
        message.contains("changed while it was resolved"),
        "{message}"
    );
    assert_eq!(
        std::fs::read_to_string(decoy.join("dir/a.md")).unwrap(),
        "old\n"
    );
}

#[test]
fn a_relative_target_or_root_is_refused() {
    let (_base, real, _aliased) = linked_project();
    let err = crate::fs::safe_fs::safe_replace_outside_workdir(Path::new("dir/a.md"), &real, b"x");
    assert!(format!("{:#}", err.unwrap_err()).contains("target dir/a.md is not absolute"));
    let file = real.join("dir/a.md");
    let err = crate::fs::safe_fs::safe_replace_outside_workdir(&file, Path::new("proj"), b"x");
    assert!(format!("{:#}", err.unwrap_err()).contains("project_root proj is not absolute"));
}
