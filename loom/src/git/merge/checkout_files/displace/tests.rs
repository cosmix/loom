use super::*;
use crate::git::merge::test_support::{init_repo, rev, stage_branch};
use std::os::unix::fs::{symlink, PermissionsExt};

/// `loom/s1` adds `u.txt` and `d/v.txt`; the checkout on `main` holds the
/// same bytes untracked. Returns the stage tip.
fn untracked_copies(root: &Path) -> (String, Vec<String>) {
    stage_branch(root, "s1", &[("u.txt", "incoming"), ("d/v.txt", "nested")]);
    std::fs::write(root.join("u.txt"), "incoming").unwrap();
    std::fs::create_dir_all(root.join("d")).unwrap();
    std::fs::write(root.join("d/v.txt"), "nested").unwrap();
    (
        rev(root, "loom/s1"),
        vec!["u.txt".to_string(), "d/v.txt".to_string()],
    )
}

fn attempt_dirs(root: &Path) -> usize {
    std::fs::read_dir(root.join(".git").join(QUARANTINE_ROOT))
        .map(|entries| entries.count())
        .unwrap_or(0)
}

fn is_absent(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_err()
}

fn incomplete(block: MergeBlock) -> (PathBuf, Vec<RestoreFailure>) {
    match block {
        MergeBlock::CheckoutRestoreIncomplete {
            quarantine,
            failures,
            ..
        } => (quarantine, failures),
        other => panic!("expected CheckoutRestoreIncomplete, got {other:?}"),
    }
}

#[test]
fn displaced_files_go_back_on_restore_and_the_quarantine_is_removed() {
    let repo = init_repo();
    let root = repo.path();
    let (tip, paths) = untracked_copies(root);

    let displaced = displace_files(root, "s1", &tip, &paths).unwrap();
    assert!(is_absent(&root.join("u.txt")));
    assert!(is_absent(&root.join("d/v.txt")));
    assert_eq!(attempt_dirs(root), 1);

    assert!(displaced.restore().is_none());
    assert_eq!(std::fs::read(root.join("u.txt")).unwrap(), b"incoming");
    assert_eq!(std::fs::read(root.join("d/v.txt")).unwrap(), b"nested");
    assert_eq!(attempt_dirs(root), 0);
}

#[test]
fn discard_removes_the_quarantine() {
    let repo = init_repo();
    let root = repo.path();
    let (tip, paths) = untracked_copies(root);

    displace_files(root, "s1", &tip, &paths).unwrap().discard();

    assert!(is_absent(&root.join("u.txt")));
    assert_eq!(attempt_dirs(root), 0);
}

#[test]
fn a_file_changed_before_the_move_stays_in_quarantine_and_blocks() {
    let repo = init_repo();
    let root = repo.path();
    let (tip, paths) = untracked_copies(root);
    inject_before_displace(|checkout: &Path| {
        std::fs::write(checkout.join("u.txt"), "operator rewrite").unwrap();
    });

    let (quarantine, failures) = incomplete(displace_files(root, "s1", &tip, &paths).unwrap_err());

    assert_eq!(
        failures,
        vec![RestoreFailure {
            path: "u.txt".to_string(),
            reason: CHANGED.to_string(),
        }]
    );
    assert_eq!(
        std::fs::read(quarantine.join("u.txt")).unwrap(),
        b"operator rewrite"
    );
    assert!(is_absent(&root.join("u.txt")));
    assert_eq!(std::fs::read(root.join("d/v.txt")).unwrap(), b"nested");
}

#[test]
fn a_file_that_cannot_be_compared_stays_in_quarantine_and_blocks() {
    let repo = init_repo();
    let root = repo.path();
    // `main` has no `u.txt`, so the blob read for the comparison fails.
    let main = rev(root, "main");
    std::fs::write(root.join("u.txt"), "operator bytes").unwrap();

    let (quarantine, failures) =
        incomplete(displace_files(root, "s1", &main, &["u.txt".to_string()]).unwrap_err());

    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].path, "u.txt");
    assert!(
        failures[0].reason.contains("could not be compared"),
        "{failures:?}"
    );
    assert!(is_absent(&root.join("u.txt")));
    assert_eq!(
        std::fs::read(quarantine.join("u.txt")).unwrap(),
        b"operator bytes"
    );
}

#[test]
fn restore_keeps_a_file_whose_path_was_reoccupied() {
    let repo = init_repo();
    let root = repo.path();
    let (tip, paths) = untracked_copies(root);
    let displaced = displace_files(root, "s1", &tip, &paths).unwrap();
    std::fs::write(root.join("u.txt"), "operator").unwrap();

    let (quarantine, failures) = incomplete(displaced.restore().unwrap());

    assert_eq!(failures.len(), 1, "{failures:?}");
    assert_eq!(failures[0].path, "u.txt");
    assert!(
        failures[0].reason.starts_with("could not be put back: "),
        "{failures:?}"
    );
    assert_eq!(std::fs::read(root.join("u.txt")).unwrap(), b"operator");
    assert_eq!(
        std::fs::read(quarantine.join("u.txt")).unwrap(),
        b"incoming"
    );
    assert_eq!(std::fs::read(root.join("d/v.txt")).unwrap(), b"nested");
}

#[test]
fn a_planted_link_at_the_quarantine_root_refuses_and_moves_nothing() {
    let repo = init_repo();
    let root = repo.path();
    let (tip, paths) = untracked_copies(root);
    let target = tempfile::TempDir::new().unwrap();
    symlink(target.path(), root.join(".git").join(QUARANTINE_ROOT)).unwrap();

    let result = displace_files(root, "s1", &tip, &paths);

    assert!(
        matches!(result, Err(MergeBlock::UncommittedOverlap { .. })),
        "{result:?}"
    );
    assert_eq!(std::fs::read(root.join("u.txt")).unwrap(), b"incoming");
    assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 0);
}

#[test]
fn a_parent_swapped_for_a_link_after_the_quarantine_opens_moves_nothing_outside() {
    let repo = init_repo();
    let root = repo.path();
    let (tip, paths) = untracked_copies(root);
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("v.txt"), "outside bytes").unwrap();
    let outside_dir = outside.path().to_path_buf();
    inject_before_displace(move |checkout: &Path| {
        std::fs::remove_dir_all(checkout.join("d")).unwrap();
        symlink(&outside_dir, checkout.join("d")).unwrap();
    });

    let result = displace_files(root, "s1", &tip, &paths);

    assert!(
        matches!(result, Err(MergeBlock::UncommittedOverlap { .. })),
        "{result:?}"
    );
    assert_eq!(
        std::fs::read(outside.path().join("v.txt")).unwrap(),
        b"outside bytes"
    );
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
    assert_eq!(std::fs::read(root.join("u.txt")).unwrap(), b"incoming");
    assert_eq!(attempt_dirs(root), 0);
}

#[test]
fn a_failure_partway_puts_back_what_moved() {
    let repo = init_repo();
    let root = repo.path();
    let main = rev(root, "main");
    let paths = vec!["a.txt".to_string(), "gone/b.txt".to_string()];

    let result = displace_files(root, "s1", &main, &paths);

    assert!(
        matches!(result, Err(MergeBlock::UncommittedOverlap { .. })),
        "{result:?}"
    );
    assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"seed");
    assert_eq!(attempt_dirs(root), 0);
}

#[test]
fn exec_bit_survives_a_round_trip() {
    let repo = init_repo();
    let root = repo.path();
    stage_branch(root, "s1", &[("run.sh", "#!/bin/sh\n")]);
    let script = root.join("run.sh");
    std::fs::write(&script, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let tip = rev(root, "loom/s1");

    let displaced = displace_files(root, "s1", &tip, &["run.sh".to_string()]).unwrap();
    assert!(displaced.restore().is_none());

    let mode = std::fs::metadata(&script).unwrap().permissions().mode();
    assert_eq!(mode & 0o111, 0o111);
}
