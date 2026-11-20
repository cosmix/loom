//! A stage's cache root goes with its worktree, and stays with a retained one.

use super::tests::{git_ok, setup_git_repo};
use super::{cleanup_after_merge, CleanupConfig};
use crate::stage_cache::StageCache;
use serial_test::serial;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// `HOME` and `XDG_CACHE_HOME` pinned to a temp dir, restored on drop.
struct CacheEnv {
    saved: Vec<(&'static str, Option<OsString>)>,
    _dir: TempDir,
}

impl CacheEnv {
    fn pin() -> Self {
        let dir = TempDir::new().unwrap();
        let saved = ["HOME", "XDG_CACHE_HOME"]
            .into_iter()
            .map(|name| (name, std::env::var_os(name)))
            .collect();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("XDG_CACHE_HOME", dir.path().join("xdg"));
        Self { saved, _dir: dir }
    }

    fn cache(&self, repo: &Path) -> StageCache {
        StageCache::resolve(
            repo,
            "stage-1",
            self._dir.path(),
            Some(&self._dir.path().join("xdg")),
        )
        .unwrap()
    }
}

impl Drop for CacheEnv {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn add_stage_worktree(repo: &Path) {
    fs::create_dir_all(repo.join(".worktrees")).unwrap();
    git_ok(
        repo,
        &[
            "worktree",
            "add",
            ".worktrees/stage-1",
            "-b",
            "loom/stage-1",
        ],
    );
}

fn seeded_cache(env: &CacheEnv, repo: &Path) -> StageCache {
    let cache = env.cache(repo);
    cache.ensure().unwrap();
    fs::write(cache.root().join("artifact"), "built").unwrap();
    cache
}

#[test]
#[serial]
fn the_cache_is_removed_after_the_worktree_is_removed() {
    let env = CacheEnv::pin();
    let repo = setup_git_repo();
    add_stage_worktree(repo.path());
    let cache = seeded_cache(&env, repo.path());

    let result = cleanup_after_merge("stage-1", repo.path(), &CleanupConfig::quiet()).unwrap();

    assert!(result.worktree_removed, "{:?}", result.warnings);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(cache.root().symlink_metadata().is_err());
}

#[test]
#[serial]
fn the_cache_is_kept_while_the_worktree_is_retained() {
    let env = CacheEnv::pin();
    let repo = setup_git_repo();
    add_stage_worktree(repo.path());
    let worktree = repo.path().join(".worktrees/stage-1");
    // The state git leaves after `worktree remove` loses a race with a late
    // write: only `.claude/.cc-writes/` remains, so ownership is unprovable.
    fs::remove_dir_all(&worktree).unwrap();
    fs::remove_dir_all(repo.path().join(".git/worktrees/stage-1")).unwrap();
    fs::create_dir_all(worktree.join(".claude/.cc-writes")).unwrap();
    let cache = seeded_cache(&env, repo.path());

    let result = cleanup_after_merge("stage-1", repo.path(), &CleanupConfig::quiet()).unwrap();

    assert!(!result.worktree_removed);
    assert!(worktree.exists());
    assert!(cache.root().join("artifact").is_file());
}
