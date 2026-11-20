//! Contracts `a-failed-trial-restores-the-frozen-file`,
//! `a-trial-never-writes-through-a-symlink` and `a-filing-needs-a-passing-trial`:
//! `with_overlay` puts replacement bytes over frozen files only for the run and
//! never through a symlink, and `receipt_covers` admits a replacement only when
//! a passing trial at the same head hashed the same bytes.
use super::trial::{receipt_covers, with_overlay, TrialFile, TrialReceipt};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::Path;
use tempfile::TempDir;

const FILE: &str = "src/t.rs";
const FROZEN: &[u8] = b"frozen";
const REPLACEMENT: &[u8] = b"replacement bytes";
const OUTSIDE: &[u8] = b"outside the worktree";

fn overlay() -> Vec<(String, Vec<u8>)> {
    vec![(FILE.to_string(), REPLACEMENT.to_vec())]
}

/// Run `with_overlay` over `root`, recording whether the run was called.
fn try_overlay(root: &Path) -> (anyhow::Result<()>, bool) {
    let mut called = false;
    let result = with_overlay(root, &overlay(), || {
        called = true;
        Ok(())
    });
    (result, called)
}

#[test]
fn a_failed_trial_restores_the_frozen_file() {
    let worktree = TempDir::new().expect("worktree");
    let path = worktree.path().join(FILE);
    fs::create_dir_all(path.parent().expect("parent")).expect("create src");
    fs::write(&path, FROZEN).expect("write frozen file");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod 0o755");

    let mut seen = Vec::new();
    let result: anyhow::Result<()> = with_overlay(worktree.path(), &overlay(), || {
        seen = fs::read(&path)?;
        anyhow::bail!("contract run failed")
    });

    assert_eq!(seen, REPLACEMENT, "the run must see the replacement bytes");
    let error = result.expect_err("the run's error must be returned");
    assert!(
        format!("{error:#}").contains("contract run failed"),
        "with_overlay must return the run's own error, got: {error:#}"
    );
    assert_eq!(fs::read(&path).expect("read restored"), FROZEN);
    let mode = fs::metadata(&path).expect("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o755, "the executable bit must be restored");
}

#[test]
fn a_trial_never_writes_through_a_symlink() {
    // A symlinked target: src/t.rs points at a file outside the worktree.
    let outside = TempDir::new().expect("outside dir");
    let target = outside.path().join("t.rs");
    fs::write(&target, OUTSIDE).expect("write outside file");
    let worktree = TempDir::new().expect("worktree");
    fs::create_dir_all(worktree.path().join("src")).expect("create src");
    symlink(&target, worktree.path().join(FILE)).expect("symlink file");

    let (result, called) = try_overlay(worktree.path());
    assert!(result.is_err(), "a symlinked target must be refused");
    assert!(!called, "the run must not start after a refusal");
    assert_eq!(fs::read(&target).expect("read outside"), OUTSIDE);
    let link = fs::symlink_metadata(worktree.path().join(FILE)).expect("link metadata");
    assert!(link.file_type().is_symlink(), "the link itself must remain");

    // A symlinked parent: src points at a directory outside the worktree.
    let parent_worktree = TempDir::new().expect("second worktree");
    symlink(outside.path(), parent_worktree.path().join("src")).expect("symlink src");

    let (result, called) = try_overlay(parent_worktree.path());
    assert!(result.is_err(), "a symlinked parent must be refused");
    assert!(!called, "the run must not start after a refusal");
    assert_eq!(fs::read(&target).expect("read outside"), OUTSIDE);
}

#[test]
fn a_filing_needs_a_passing_trial() {
    let head = "a".repeat(40);
    let sha = "1".repeat(64);
    let receipt = TrialReceipt {
        head: head.clone(),
        files: vec![TrialFile {
            path: FILE.to_string(),
            sha256: sha.clone(),
        }],
    };
    let replacements = vec![(FILE.to_string(), sha.clone())];

    let missing = receipt_covers(None, &head, &replacements).expect_err("no receipt");
    assert!(
        missing.contains("loom check") && missing.contains("--replacement"),
        "the refusal must name the trial command, got: {missing}"
    );
    let other_head = "b".repeat(40);
    assert!(
        receipt_covers(Some(&receipt), &other_head, &replacements).is_err(),
        "a receipt for another head must not cover the filing"
    );
    let other_bytes = vec![(FILE.to_string(), "2".repeat(64))];
    assert!(
        receipt_covers(Some(&receipt), &head, &other_bytes).is_err(),
        "a receipt for other bytes must not cover the filing"
    );
    assert_eq!(receipt_covers(Some(&receipt), &head, &replacements), Ok(()));
}
