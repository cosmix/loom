//! The macOS worker lifecycle fixture: the production `run_from_dir` against
//! an installed Codex, in a hermetic scratch with fixture credentials only.
//! `seatbelt_profile_tests::native_macos_worker_lifecycle` asserts what
//! [`run_lifecycle`] measures. Callers are `#[serial_test::serial]`: the
//! fixture pins `HOME` and `PATH`, which are process-wide.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::native_worker::ids::{mint_invocation_id, EOF_GRACE_SECS, INTERRUPT_GRACE_SECS};
use crate::native_worker::machine::STARTUP_DEADLINE_SECS;
use crate::native_worker::model::{NativeManifest, Ownership};
use crate::native_worker::{index, store};
use crate::test_cache_env::{repo_with_work_dir, CacheEnv};

const STAGE: &str = "native-lifecycle";
const UNIT: &str = "w1";
/// The worker's own deadline: long enough for an offline initialize and
/// thread, short enough that a turn which cannot reach the model ends.
const DEADLINE_SECS: u64 = 60;
const BRIEF: &[u8] = b"Reply with the single word done.\n";
/// A fixture credential: never a real key, never printed.
const FIXTURE_AUTH: &str = r#"{"OPENAI_API_KEY":"sk-loom-fixture-not-a-credential"}"#;

/// What one lifecycle run showed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LifecycleReport {
    /// `run_from_dir` returned within the startup deadline, the worker
    /// deadline, its interrupt and EOF graces and the 30 s kill bound.
    pub(crate) exited_within_bound: bool,
    /// The record ends with ownership `Released`.
    pub(crate) ownership_released: bool,
    /// The private invocation root `boundary.json` named is gone.
    pub(crate) private_root_removed: bool,
}

/// Runs one worker through the production supervisor with `codex_program`
/// as the Codex executable and measures the three outcomes.
pub(crate) fn run_lifecycle(codex_program: &Path) -> LifecycleReport {
    let loom = crate::native_worker::boundary::loom_program::built_loom()
        .expect("the lifecycle run needs the built loom beside the test binary: run cargo build");
    let _loom = crate::native_worker::boundary::loom_program::TestOverride::set(loom);
    let _cache = CacheEnv::pin();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    let (repo, work_dir) = repo_with_work_dir();
    let worktree = repo.path().canonicalize().expect("canonical worktree");
    write_fixture_home(&home);
    let bin = home.join("fixture-bin");
    std::fs::create_dir_all(&bin).expect("fixture bin");
    std::os::unix::fs::symlink(codex_program, bin.join("codex")).expect("codex link");
    let _path = PathGuard::prepend(&bin);
    let dir = admitted_record(&work_dir, &worktree);
    let started = Instant::now();
    let result = super::run_from_dir(&dir);
    let elapsed = started.elapsed();
    let bound = Duration::from_secs(
        STARTUP_DEADLINE_SECS + DEADLINE_SECS + INTERRUPT_GRACE_SECS + EOF_GRACE_SECS + 30 + 30,
    );
    if let Err(error) = &result {
        eprintln!("run_from_dir refused: {error:#}");
    }
    let invocation = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let ownership = index::unit_state(&work_dir, STAGE, UNIT)
        .ok()
        .flatten()
        .map(|state| state.ownership);
    let root = store::read_boundary(&dir)
        .ok()
        .flatten()
        .and_then(|identity| identity.mac)
        .filter(|mac| mac.invocation == invocation)
        .map(|mac| mac.scratch_root);
    LifecycleReport {
        exited_within_bound: result.is_ok() && elapsed <= bound,
        ownership_released: ownership == Some(Ownership::Released),
        private_root_removed: root.is_some_and(|root| std::fs::symlink_metadata(root).is_err()),
    }
}

/// `$HOME/.codex` with the fixture `auth.json` and an `AGENTS.md`, private.
fn write_fixture_home(home: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let codex_home = home.join(".codex");
    std::fs::create_dir_all(codex_home.join("sessions")).expect("fixture codex home");
    std::fs::set_permissions(&codex_home, std::fs::Permissions::from_mode(0o700))
        .expect("codex home mode");
    let auth = codex_home.join("auth.json");
    std::fs::write(&auth, FIXTURE_AUTH).expect("fixture auth.json");
    std::fs::set_permissions(&auth, std::fs::Permissions::from_mode(0o600)).expect("auth mode");
    std::fs::write(codex_home.join("AGENTS.md"), "Fixture instructions.\n").expect("AGENTS.md");
}

/// An invocation record as admission leaves it, under `work_dir`.
fn admitted_record(work_dir: &Path, worktree: &Path) -> PathBuf {
    let manifest = NativeManifest {
        v: 1,
        stage_id: STAGE.to_string(),
        unit_id: UNIT.to_string(),
        invocation_id: mint_invocation_id().expect("invocation id"),
        owner_session_id: "session-lifecycle".to_string(),
        workspace_root: worktree.to_path_buf(),
        brief_path: worktree.join("brief.md"),
        brief_sha256: crate::utils::sha256_hex(BRIEF),
        model: "gpt-6.1-terra".to_string(),
        effort: "low".to_string(),
        deadline_secs: DEADLINE_SECS,
        created_at: Utc::now(),
    };
    store::create_invocation(work_dir, &manifest, BRIEF).expect("create invocation")
}

/// `PATH` with one directory first, restored on drop.
struct PathGuard(Option<OsString>);

impl PathGuard {
    fn prepend(dir: &Path) -> Self {
        let saved = std::env::var_os("PATH");
        let mut entries = vec![dir.to_path_buf()];
        entries.extend(std::env::split_paths(&saved.clone().unwrap_or_default()));
        std::env::set_var("PATH", std::env::join_paths(entries).expect("PATH"));
        Self(saved)
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        match &self.0 {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}
