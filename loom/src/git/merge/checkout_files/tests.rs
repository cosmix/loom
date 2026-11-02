use super::*;
use crate::git::merge::test_support::{commit_file, git_ok, git_out, init_repo};
use std::cell::Cell;
use std::cell::RefCell;
use std::os::unix::fs::symlink;

type BeforeRestore = Box<dyn FnOnce(&Path)>;

thread_local! {
    static BEFORE_RESTORE: RefCell<Option<BeforeRestore>> = RefCell::new(None);
    static FAIL_WRITE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn fail_restored_write() -> bool {
    FAIL_WRITE.with(|flag| flag.replace(false))
}

pub(super) fn before_restore(repo: &Path) {
    BEFORE_RESTORE.with(|hook| {
        if let Some(hook) = hook.borrow_mut().take() {
            hook(repo);
        }
    });
}

pub(super) fn inject_before_restore(hook: BeforeRestore) {
    BEFORE_RESTORE.with(|slot| *slot.borrow_mut() = Some(hook));
}

#[test]
fn a_failed_removal_restores_the_files_already_removed() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "x.txt", "bytes", "x");
    std::fs::create_dir(root.join("d")).unwrap();

    let paths = vec!["x.txt".to_string(), "d".to_string()];
    let error = remove_files(root, "HEAD", &paths).unwrap_err();

    assert!(error.to_string().contains('d'), "{error:#}");
    assert_eq!(
        std::fs::read_to_string(root.join("x.txt")).unwrap(),
        "bytes"
    );
}

#[test]
fn a_failed_removal_reports_a_recreated_file_without_overwriting_it() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "first.txt", "original", "first");
    std::fs::create_dir(root.join("cannot-remove")).unwrap();
    inject_before_restore(Box::new(|repo| {
        std::fs::write(repo.join("first.txt"), "operator").unwrap();
    }));

    let error =
        remove_files(root, "HEAD", &["first.txt".into(), "cannot-remove".into()]).unwrap_err();

    assert_eq!(error.restore.failures.len(), 1);
    assert_eq!(error.restore.failures[0].path, "first.txt");
    assert_eq!(std::fs::read(root.join("first.txt")).unwrap(), b"operator");
}

#[test]
fn restoring_keeps_the_exec_bit_of_the_blob() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "run.sh", "#!/bin/sh\n", "script");
    git_ok(root, &["update-index", "--chmod=+x", "run.sh"]);
    git_ok(root, &["commit", "-m", "exec"]);
    commit_file(root, "plain.txt", "plain", "plain");
    let paths = vec!["run.sh".to_string(), "plain.txt".to_string()];
    remove_files(root, "HEAD", &paths).unwrap();
    assert!(!root.join("run.sh").exists());

    assert!(restore_files(root, "HEAD", &paths).failures.is_empty());

    let mode = |name: &str| {
        std::fs::metadata(root.join(name))
            .unwrap()
            .permissions()
            .mode()
    };
    assert_eq!(mode("run.sh") & 0o111, 0o111);
    assert_eq!(mode("plain.txt") & 0o111, 0);
    assert_eq!(
        std::fs::read_to_string(root.join("run.sh")).unwrap(),
        "#!/bin/sh\n"
    );
}

#[test]
fn restoration_refuses_an_occupied_file_and_a_symlink() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "regular.txt", "original", "regular");
    commit_file(root, "linked.txt", "original", "linked");
    std::fs::write(root.join("regular.txt"), "operator").unwrap();
    std::fs::write(root.join("outside.txt"), "outside").unwrap();
    std::fs::remove_file(root.join("linked.txt")).unwrap();
    symlink("outside.txt", root.join("linked.txt")).unwrap();

    let paths = vec!["regular.txt".to_string(), "linked.txt".to_string()];
    let report = restore_files(root, "HEAD", &paths);

    assert_eq!(report.failures.len(), 2);
    assert_eq!(report.failures[0].path, "regular.txt");
    assert_eq!(report.failures[1].path, "linked.txt");
    assert_eq!(
        std::fs::read(root.join("regular.txt")).unwrap(),
        b"operator"
    );
    assert_eq!(std::fs::read(root.join("outside.txt")).unwrap(), b"outside");
    assert_eq!(
        std::fs::read_link(root.join("linked.txt")).unwrap(),
        Path::new("outside.txt")
    );
}

#[test]
fn restoration_reports_missing_blobs_without_creating_a_file() {
    let repo = init_repo();
    let root = repo.path();
    let report = restore_files(root, "HEAD", &["missing.txt".to_string()]);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].path, "missing.txt");
    assert!(report.failures[0].reason.contains("mode unavailable"));
    assert!(!root.join("missing.txt").exists());
}

#[test]
fn restoration_reports_a_blob_read_failure() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "blob.txt", "original", "blob");
    std::fs::remove_file(root.join("blob.txt")).unwrap();
    let oid = git_out(root, &["rev-parse", "HEAD:blob.txt"]);
    let oid = oid.trim();
    std::fs::remove_file(root.join(".git/objects").join(&oid[..2]).join(&oid[2..])).unwrap();

    let report = restore_files(root, "HEAD", &["blob.txt".to_string()]);

    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].path, "blob.txt");
    assert!(report.failures[0].reason.contains("blob unavailable"));
    assert!(!root.join("blob.txt").exists());
}

#[test]
fn restoration_reports_a_write_failure_and_keeps_the_partial_file_visible() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "blob.txt", "original", "blob");
    std::fs::remove_file(root.join("blob.txt")).unwrap();
    FAIL_WRITE.with(|flag| flag.set(true));

    let report = restore_files(root, "HEAD", &["blob.txt".to_string()]);

    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0]
        .reason
        .contains("simulated restored-file write failure"));
    assert_eq!(std::fs::read(root.join("blob.txt")).unwrap(), b"");
}

#[test]
fn restoration_does_not_follow_a_replaced_parent_directory() {
    let repo = init_repo();
    let root = repo.path();
    std::fs::create_dir(root.join("folder")).unwrap();
    commit_file(root, "folder/file.txt", "original", "nested");
    std::fs::remove_file(root.join("folder/file.txt")).unwrap();
    std::fs::rename(root.join("folder"), root.join("old-folder")).unwrap();
    std::fs::create_dir(root.join("outside")).unwrap();
    symlink("outside", root.join("folder")).unwrap();

    let report = restore_files(root, "HEAD", &["folder/file.txt".to_string()]);

    assert_eq!(report.failures.len(), 1);
    assert!(!root.join("outside/file.txt").exists());
}

#[test]
fn the_probe_reads_the_disk_and_compares_bytes() {
    let repo = init_repo();
    let root = repo.path();
    commit_file(root, "blob.txt", "same", "blob");
    std::fs::create_dir(root.join("dir")).unwrap();
    symlink("a.txt", root.join("link")).unwrap();
    let probe = DiskProbe {
        repo: root,
        base: "HEAD~1",
        tree: "HEAD",
    };

    assert_eq!(probe.disk("blob.txt"), DiskEntry::RegularFile);
    assert_eq!(probe.disk("dir"), DiskEntry::Directory);
    assert_eq!(probe.disk("link"), DiskEntry::Other);
    assert_eq!(probe.disk("missing"), DiskEntry::Absent);
    assert_eq!(probe.disk("a.txt/below"), DiskEntry::Absent);
    assert!(probe.equals_blob("blob.txt"));
    std::fs::write(root.join("blob.txt"), "different").unwrap();
    assert!(!probe.equals_blob("blob.txt"));
    assert!(!probe.equals_blob("link"), "a symlink never equals a blob");
    assert!(probe.in_base_tree("a.txt"));
    assert!(!probe.in_base_tree("blob.txt"));
}
