//! Shared fixtures for tests that resolve a stage's build cache, which lives
//! below the process's `HOME` and `XDG_CACHE_HOME`. Every user is
//! `#[serial_test::serial]`: the variables are process-wide.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use tempfile::TempDir;

use crate::stage_cache::StageCache;
use crate::test_git::TestRepo;

/// The variables a pin sets.
const PINNED_VARS: [&str; 2] = ["HOME", "XDG_CACHE_HOME"];

/// Every alive [`CacheEnv`]'s `HOME`, oldest first, and the environment from
/// before the oldest. Each guard restoring what it saw would restore in drop
/// order, and a collection drops first to last: the second would restore
/// `HOME` to the first one's deleted temp dir, which every later test in the
/// process then reads. A drop instead pins the newest guard still alive, or
/// restores the environment from before the oldest.
struct Pins {
    outer: Vec<(&'static str, Option<OsString>)>,
    alive: Vec<PathBuf>,
}

static PINS: Mutex<Pins> = Mutex::new(Pins {
    outer: Vec::new(),
    alive: Vec::new(),
});

/// The pins, whatever test panicked while holding them.
fn pins() -> MutexGuard<'static, Pins> {
    PINS.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `HOME` pinned to a temp dir and `XDG_CACHE_HOME` unset, restored once the
/// last alive pin drops, in any drop order ([`Pins`]).
pub(crate) struct CacheEnv {
    home: TempDir,
}

impl CacheEnv {
    pub(crate) fn pin() -> Self {
        let home = TempDir::new().unwrap();
        let mut pins = pins();
        if pins.alive.is_empty() {
            pins.outer = PINNED_VARS
                .map(|name| (name, std::env::var_os(name)))
                .to_vec();
        }
        pins.alive.push(home.path().to_path_buf());
        set_home(home.path());
        Self { home }
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
        let mut pins = pins();
        pins.alive.retain(|home| home != self.home.path());
        if let Some(newest) = pins.alive.last() {
            set_home(newest);
            return;
        }
        for (name, value) in std::mem::take(&mut pins.outer) {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

fn set_home(home: &Path) {
    std::env::set_var("HOME", home);
    std::env::remove_var("XDG_CACHE_HOME");
}

/// A repository with a state directory `<repo>/.loom/work`.
pub(crate) fn repo_with_work_dir() -> (TestRepo, PathBuf) {
    let repo = TestRepo::new();
    let work_dir = repo.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    (repo, work_dir)
}

#[test]
#[serial_test::serial]
fn pins_dropped_first_to_last_restore_the_outer_home() {
    let outside = std::env::var_os("HOME");
    let home = |env: &CacheEnv| Some(env.home.path().as_os_str().to_owned());
    let pinned = [CacheEnv::pin(), CacheEnv::pin()];
    assert_eq!(std::env::var_os("HOME"), home(&pinned[1]));
    let [first, second] = pinned;
    let second_home = home(&second);
    drop(first);
    assert_eq!(
        std::env::var_os("HOME"),
        second_home,
        "the newest alive pin holds"
    );
    drop(second);
    assert_eq!(
        std::env::var_os("HOME"),
        outside,
        "the last drop restores HOME"
    );
}
