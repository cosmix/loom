use super::{list, note, show};
use serial_test::serial;
use std::env;
use std::process::Command;
use tempfile::TempDir;

/// Create a temp dir with a real `git init`-ed repo. Required because
/// `get_or_create_work_dir`/`find_repo_root_from_cwd` only trust a
/// candidate root that actually has a `.git` entry.
fn init_git_repo() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let run_git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(temp_dir.path())
            .output()
            .unwrap()
    };
    run_git(&["init", "--initial-branch=main"]);
    run_git(&["config", "user.email", "test@test.com"]);
    run_git(&["config", "user.name", "Test"]);
    temp_dir
}

/// Restores cwd and `LOOM_STAGE_ID` on drop. Tests mutate process-global
/// state (cwd, env vars) so `#[serial]` plus this guard keep them isolated.
struct EnvGuard {
    original_dir: std::path::PathBuf,
    original_stage_id: Option<String>,
}

impl EnvGuard {
    fn new() -> Self {
        Self {
            original_dir: env::current_dir().unwrap(),
            original_stage_id: env::var("LOOM_STAGE_ID").ok(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        env::set_current_dir(&self.original_dir).unwrap();
        match &self.original_stage_id {
            Some(v) => env::set_var("LOOM_STAGE_ID", v),
            None => env::remove_var("LOOM_STAGE_ID"),
        }
    }
}

#[test]
#[serial]
fn note_creates_work_dir_when_missing_using_ad_hoc_stage() {
    let _guard = EnvGuard::new();
    env::remove_var("LOOM_STAGE_ID");
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    assert!(!repo.path().join(".work").exists());

    note("probe text".to_string(), None).unwrap();

    let journal_path = repo.path().join(".work/memory/ad-hoc.md");
    assert!(
        journal_path.exists(),
        ".work/memory/ad-hoc.md should be auto-created"
    );
    let content = std::fs::read_to_string(&journal_path).unwrap();
    assert!(content.contains("probe text"));
}

#[test]
#[serial]
fn note_uses_loom_stage_id_env_var_over_sentinel() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    env::set_var("LOOM_STAGE_ID", "env-stage");

    note("from env".to_string(), None).unwrap();

    assert!(repo.path().join(".work/memory/env-stage.md").exists());
    assert!(!repo.path().join(".work/memory/ad-hoc.md").exists());
}

#[test]
#[serial]
fn note_explicit_stage_overrides_env_var() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    env::set_var("LOOM_STAGE_ID", "env-stage");

    note("explicit wins".to_string(), Some("cli-stage".to_string())).unwrap();

    assert!(repo.path().join(".work/memory/cli-stage.md").exists());
    assert!(!repo.path().join(".work/memory/env-stage.md").exists());
}

#[test]
#[serial]
fn note_outside_git_repo_still_fails() {
    let _guard = EnvGuard::new();
    env::remove_var("LOOM_STAGE_ID");
    // A plain temp dir (no `git init`) has no `.git` anywhere in its
    // ancestry, so this must fail exactly like the pre-existing behavior.
    let plain_dir = TempDir::new().unwrap();
    env::set_current_dir(plain_dir.path()).unwrap();

    let result = note("should not be recorded".to_string(), None);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains(".work directory not found"));
    assert!(!plain_dir.path().join(".work").exists());
}

#[test]
#[serial]
fn list_and_show_degrade_without_creating_work_dir() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();

    assert!(list(None, None).is_ok());
    assert!(
        !repo.path().join(".work").exists(),
        "list must not create .work"
    );

    assert!(show(None, true).is_ok());
    assert!(
        !repo.path().join(".work").exists(),
        "show --all must not create .work"
    );
}

#[test]
#[serial]
fn note_reuses_existing_work_dir_without_recreating() {
    let _guard = EnvGuard::new();
    env::remove_var("LOOM_STAGE_ID");
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    // Pre-existing `.work` (as a real loom plan would leave behind) must
    // be found by `get_work_dir()` and reused, not recreated.
    std::fs::create_dir_all(repo.path().join(".work")).unwrap();

    note("reuse me".to_string(), None).unwrap();

    let journal_path = repo.path().join(".work/memory/ad-hoc.md");
    assert!(journal_path.exists());
    let content = std::fs::read_to_string(&journal_path).unwrap();
    assert!(content.contains("reuse me"));
}
