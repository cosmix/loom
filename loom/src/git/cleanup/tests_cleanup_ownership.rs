//! Batch cleanup must establish ownership before draining any spool bytes.

use std::fs;
use std::path::{Path, PathBuf};

use super::tests::{git_ok, setup_git_repo};
use super::{cleanup_after_merge, cleanup_worktree, CleanupConfig};
use crate::fs::memory::{append_to_spool, MemoryEntry, MemoryEntryType, SPOOL_RELPATH};
use crate::telemetry::TELEMETRY_SPOOL_RELPATH;
use tempfile::TempDir;

const SETTINGS: &str = ".claude/settings.json";
const TRACKED: [&str; 3] = [SETTINGS, SPOOL_RELPATH, TELEMETRY_SPOOL_RELPATH];

fn add_worktree(root: &Path) -> PathBuf {
    git_ok(
        root,
        &[
            "worktree",
            "add",
            ".worktrees/stage-1",
            "-b",
            "loom/stage-1",
        ],
    );
    root.join(".worktrees/stage-1")
}

fn tracked_spool_repo() -> (TempDir, PathBuf) {
    let repo = setup_git_repo();
    fs::create_dir_all(repo.path().join(".loom/work")).unwrap();
    crate::git::worktree::capture_main_identity(repo.path(), &repo.path().join(".loom/work"))
        .unwrap();
    fs::create_dir_all(repo.path().join(".claude")).unwrap();
    for path in TRACKED {
        fs::write(repo.path().join(path), "committed bytes\n").unwrap();
        git_ok(repo.path(), &["add", "-f", path]);
    }
    git_ok(
        repo.path(),
        &["commit", "-m", "tracked scaffold and spools"],
    );
    let worktree = add_worktree(repo.path());
    for path in TRACKED {
        fs::write(worktree.join(path), format!("local edits: {path}\n")).unwrap();
    }
    std::os::unix::fs::symlink(repo.path().join(".loom/work"), worktree.join(".loom/work"))
        .unwrap();
    (repo, worktree)
}

fn assert_local_bytes(worktree: &Path) {
    for path in TRACKED {
        assert_eq!(
            fs::read_to_string(worktree.join(path)).unwrap(),
            format!("local edits: {path}\n")
        );
    }
}

#[test]
fn corrupt_index_preserves_tracked_spools_for_direct_and_batch_cleanup() {
    let (repo, worktree) = tracked_spool_repo();
    fs::write(
        repo.path().join(".git/worktrees/stage-1/index"),
        "broken index",
    )
    .unwrap();

    for batch in [false, true] {
        let error = if batch {
            cleanup_after_merge("stage-1", repo.path(), &CleanupConfig::quiet()).unwrap_err()
        } else {
            cleanup_worktree("stage-1", repo.path(), false).unwrap_err()
        };
        assert!(format!("{error:#}").contains("tracked worktree scaffold"));
        assert_local_bytes(&worktree);
        assert_eq!(
            fs::read_link(worktree.join(".loom/work")).unwrap(),
            repo.path().join(".loom/work")
        );
    }
}

#[test]
fn batch_cleanup_never_drains_tracked_spool_edits() {
    let (repo, worktree) = tracked_spool_repo();

    assert!(cleanup_after_merge("stage-1", repo.path(), &CleanupConfig::quiet()).is_err());

    assert_local_bytes(&worktree);
    assert!(!repo.path().join(".loom/work/memory/stage-1.md").exists());
}

#[test]
fn normal_batch_cleanup_drains_owned_memory_before_removing_the_worktree() {
    let repo = setup_git_repo();
    let state = repo.path().join(".loom/work");
    fs::create_dir_all(&state).unwrap();
    crate::git::worktree::capture_main_identity(repo.path(), &state).unwrap();
    let worktree = add_worktree(repo.path());
    let note = MemoryEntry::new(MemoryEntryType::Note, "preserve this note".to_string());
    append_to_spool(&worktree, &note).unwrap();

    let result = cleanup_after_merge("stage-1", repo.path(), &CleanupConfig::quiet()).unwrap();

    assert!(result.worktree_removed);
    assert!(result.branch_deleted);
    assert!(!worktree.exists());
    assert!(fs::read_to_string(state.join("memory/stage-1.md"))
        .unwrap()
        .contains("preserve this note"));
}

#[test]
fn direct_cleanup_preserves_nonempty_memory_and_telemetry_spools() {
    for spool in [SPOOL_RELPATH, TELEMETRY_SPOOL_RELPATH] {
        let repo = setup_git_repo();
        let worktree = add_worktree(repo.path());
        fs::create_dir(worktree.join(".loom")).unwrap();
        fs::write(worktree.join(spool), "pending records\n").unwrap();

        let error = cleanup_worktree("stage-1", repo.path(), false).unwrap_err();

        assert!(error.to_string().contains("undrained"));
        assert_eq!(
            fs::read_to_string(worktree.join(spool)).unwrap(),
            "pending records\n"
        );
    }
}

#[test]
fn tracked_scaffold_symlink_edits_remain_for_git_to_judge() {
    for link in [".claude", ".loom", ".loom/work", ".work"] {
        let repo = setup_git_repo();
        let path = repo.path().join(link);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink("original-target", &path).unwrap();
        git_ok(repo.path(), &["add", "-f", link]);
        git_ok(repo.path(), &["commit", "-m", "tracked scaffold link"]);
        let worktree = add_worktree(repo.path());
        let changed = worktree.join(link);
        fs::remove_file(&changed).unwrap();
        std::os::unix::fs::symlink("local-target", &changed).unwrap();

        super::worktree::remove_worktree_scaffold(&worktree, repo.path()).unwrap();
        assert!(cleanup_worktree("stage-1", repo.path(), false).is_err());

        assert_eq!(fs::read_link(&changed).unwrap(), Path::new("local-target"));
    }
}

#[test]
fn a_loom_directory_symlink_never_redirects_scaffold_deletion() {
    let repo = setup_git_repo();
    let worktree = add_worktree(repo.path());
    let outside = repo.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("memory-spool.jsonl"), "").unwrap();
    std::os::unix::fs::symlink("state", outside.join("work")).unwrap();
    std::os::unix::fs::symlink(&outside, worktree.join(".loom")).unwrap();

    super::worktree::remove_worktree_scaffold(&worktree, repo.path()).unwrap();

    assert!(outside.join("memory-spool.jsonl").is_file());
    assert_eq!(
        fs::read_link(outside.join("work")).unwrap(),
        Path::new("state")
    );
    assert_eq!(fs::read_link(worktree.join(".loom")).unwrap(), outside);
}

fn redirect_checkout_pointer(worktree: &Path) -> TempDir {
    let redirected = setup_git_repo();
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", redirected.path().join(".git").display()),
    )
    .unwrap();
    redirected
}

#[test]
fn rewritten_git_pointer_cannot_hide_tracked_scaffold_ownership() {
    let (repo, worktree) = tracked_spool_repo();
    let _redirected = redirect_checkout_pointer(&worktree);

    let tracked = super::worktree::tracked_scaffold_paths(&worktree, repo.path()).unwrap();
    assert!(TRACKED.iter().all(|path| tracked.contains(*path)));
    assert!(cleanup_after_merge("stage-1", repo.path(), &CleanupConfig::quiet()).is_err());
    assert_local_bytes(&worktree);
}

#[test]
fn cleanup_diagnostics_use_the_registered_index_after_git_pointer_replacement() {
    let (repo, worktree) = tracked_spool_repo();
    let _redirected = redirect_checkout_pointer(&worktree);

    let error = super::worktree_failure::removal_failure(
        &worktree,
        repo.path(),
        anyhow::anyhow!("contains modified or untracked files"),
    );

    assert!(format!("{error:#}").contains(" M .claude/settings.json"));
    assert_local_bytes(&worktree);
}
