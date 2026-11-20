//! Shared fixtures for tests that resolve a stage's build cache, which lives
//! below the process's `HOME` and `XDG_CACHE_HOME`. Every user is
//! `#[serial_test::serial]`: the variables are process-wide.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::stage_cache::StageCache;
use crate::test_git::TestRepo;

/// `HOME` pinned to a temp dir and `XDG_CACHE_HOME` unset, restored on drop.
pub(crate) struct CacheEnv {
    saved: [(&'static str, Option<OsString>); 2],
    home: TempDir,
}

impl CacheEnv {
    pub(crate) fn pin() -> Self {
        let home = TempDir::new().unwrap();
        let saved = ["HOME", "XDG_CACHE_HOME"].map(|name| (name, std::env::var_os(name)));
        std::env::set_var("HOME", home.path());
        std::env::remove_var("XDG_CACHE_HOME");
        Self { saved, home }
    }

    /// The root `stage_id` of the repository at `repo_root` resolves to under
    /// the pinned `HOME`, computed without the helpers under test. The state
    /// directory's repository root is canonical, so `repo_root` is too.
    pub(crate) fn root_of(&self, repo_root: &Path, stage_id: &str) -> PathBuf {
        let repo_root = repo_root.canonicalize().unwrap();
        StageCache::resolve(&repo_root, stage_id, self.home.path(), None)
            .unwrap()
            .root()
            .to_path_buf()
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

/// A repository with a state directory `<repo>/.loom/work`.
pub(crate) fn repo_with_work_dir() -> (TestRepo, PathBuf) {
    let repo = TestRepo::new();
    let work_dir = repo.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    (repo, work_dir)
}
