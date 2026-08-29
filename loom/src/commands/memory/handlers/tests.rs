use super::read::{read_journal_with_pending, spool_only_stage_with_pending};
use super::{list, note, show};
use crate::fs::memory::{append_to_spool, MemoryEntry, MemoryEntryType};
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

/// A stage that can write `.work` directly (no sandbox, e.g. a main-repo
/// knowledge stage) must be refused just as firmly as the spool path when
/// its explicit `--stage` disagrees with `LOOM_STAGE_ID` - attribution must
/// not be spoofable via CLI flag regardless of which write path a call
/// takes. This replaces the old "explicit stage silently overrides env"
/// behavior, which was exactly the forged-attribution hole this closes.
#[test]
#[serial]
fn note_explicit_stage_mismatch_is_refused_direct_path() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();
    env::set_var("LOOM_STAGE_ID", "env-stage");

    let result = note(
        "attempted forgery".to_string(),
        Some("cli-stage".to_string()),
    );

    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(message.contains("does not match"), "{message}");
    assert!(message.contains("NOT recorded"), "{message}");
    assert!(!repo.path().join(".work/memory/cli-stage.md").exists());
    assert!(!repo.path().join(".work/memory/env-stage.md").exists());
}

/// With no `LOOM_STAGE_ID` (ad-hoc/interactive/operator use), there is no
/// session identity to forge, so `--stage` must remain freely usable - the
/// forgery guard is a no-op here by design.
#[test]
#[serial]
fn note_explicit_stage_allowed_when_loom_stage_id_unset() {
    let _guard = EnvGuard::new();
    env::remove_var("LOOM_STAGE_ID");
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();

    note("explicit wins".to_string(), Some("cli-stage".to_string())).unwrap();

    assert!(repo.path().join(".work/memory/cli-stage.md").exists());
}

// `AmbientTempRootGuard` and its regression test
// (`note_does_not_adopt_an_impostor_git_dir_at_the_temp_root`) live in
// `impostor_git_dir_tests.rs`, split out to keep this file under the
// maintainability limit.
#[path = "impostor_git_dir_tests.rs"]
mod impostor_git_dir_tests;

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

#[test]
#[serial]
fn note_success_takes_direct_path_and_writes_no_spool_file() {
    let _guard = EnvGuard::new();
    env::remove_var("LOOM_STAGE_ID");
    let repo = init_git_repo();
    env::set_current_dir(repo.path()).unwrap();

    note("direct path works".to_string(), None).unwrap();

    assert!(repo.path().join(".work/memory/ad-hoc.md").exists());
    assert!(
        !repo.path().join(".loom/memory-spool.jsonl").exists(),
        "a successful direct write must not fall back to the spool"
    );
}

/// The same forgery guard, exercised from inside a worktree, confirms it
/// fires before `get_or_create_work_dir`/`append_entry` are even reached -
/// no write-denial simulation needed, since the check now runs up front
/// regardless of which write path a call would otherwise take.
#[test]
#[serial]
fn note_explicit_stage_mismatch_is_refused_inside_worktree() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    let worktree_stage = "real-stage";
    let worktree_root = repo.path().join(".worktrees").join(worktree_stage);
    std::fs::create_dir_all(&worktree_root).unwrap();

    env::set_current_dir(&worktree_root).unwrap();
    env::set_var("LOOM_STAGE_ID", worktree_stage);

    let result = note(
        "attempted forgery".to_string(),
        Some("fake-stage".to_string()),
    );

    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(message.contains("does not match"), "{message}");
    assert!(message.contains("NOT recorded"), "{message}");
    assert!(
        !worktree_root.join(".loom/memory-spool.jsonl").exists(),
        "a refused forged stage claim must not spool anything"
    );
    assert!(
        !worktree_root.join(".work").exists(),
        "the guard must fire before any work_dir resolution/creation is attempted"
    );
}

#[test]
#[serial]
fn read_journal_with_pending_surfaces_a_pending_entry() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    let stage = "pending-stage";
    let worktree_root = repo.path().join(".worktrees").join(stage);
    let work_dir = worktree_root.join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();
    env::set_current_dir(&worktree_root).unwrap();
    env::set_var("LOOM_STAGE_ID", stage);

    // Seed the spool directly, standing in for an entry the daemon hasn't
    // drained into the journal file yet.
    append_to_spool(
        &worktree_root,
        &MemoryEntry::new(MemoryEntryType::Note, "still pending".to_string()),
    )
    .unwrap();

    let journal = read_journal_with_pending(&work_dir, stage).unwrap();

    assert_eq!(journal.entries.len(), 1);
    assert_eq!(journal.entries[0].content, "still pending");
}

/// `show --all` must surface a stage whose only entries are still in the
/// spool - `list_journals` enumerates journal *files*, so a stage that has
/// never had a direct write succeed (every entry so far spooled) would
/// otherwise never appear in the aggregate listing.
///
/// `show`/`list` print directly to stdout with no return value to inspect,
/// and this crate has no stdout-capture test tooling, so this asserts
/// against `spool_only_stage_with_pending` - the exact function
/// `show_all_journals` calls to decide whether to fold a stage into the
/// listing - rather than parsing captured output. The `show(None, true)`
/// call alongside it is a smoke test that the same scenario doesn't error
/// end-to-end.
#[test]
#[serial]
fn show_all_surfaces_a_spool_only_stage() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    let stage = "spool-only-stage";
    let worktree_root = repo.path().join(".worktrees").join(stage);
    let work_dir = worktree_root.join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();
    env::set_current_dir(&worktree_root).unwrap();
    env::set_var("LOOM_STAGE_ID", stage);

    // No journal file for `stage` exists at all - only a spooled entry.
    append_to_spool(
        &worktree_root,
        &MemoryEntry::new(MemoryEntryType::Note, "spool only".to_string()),
    )
    .unwrap();
    assert!(!work_dir.join("memory").join(format!("{stage}.md")).exists());

    let journals: Vec<String> = Vec::new();
    let surfaced = spool_only_stage_with_pending(&journals);
    assert_eq!(surfaced, Some(stage.to_string()));

    assert!(show(None, true).is_ok());
}

/// `loom memory list` (no `--stage`) is the command CLAUDE.md's post-compaction
/// recovery flow actually names, so this is the most important instance of
/// the three list_journals-only-enumerates-files gaps: a stage whose only
/// entries are still spooled must not be invisible to a plain `loom memory
/// list` right after recording. Same stdout-capture limitation as
/// `show_all_surfaces_a_spool_only_stage` applies, so this asserts against
/// `spool_only_stage_with_pending` plus a `list(None, None).is_ok()` smoke
/// test rather than parsed output.
#[test]
#[serial]
fn list_surfaces_a_spool_only_stage() {
    let _guard = EnvGuard::new();
    let repo = init_git_repo();
    let stage = "spool-only-list-stage";
    let worktree_root = repo.path().join(".worktrees").join(stage);
    let work_dir = worktree_root.join(".work");
    std::fs::create_dir_all(&work_dir).unwrap();
    env::set_current_dir(&worktree_root).unwrap();
    env::set_var("LOOM_STAGE_ID", stage);

    append_to_spool(
        &worktree_root,
        &MemoryEntry::new(MemoryEntryType::Note, "spool only, list path".to_string()),
    )
    .unwrap();
    assert!(!work_dir.join("memory").join(format!("{stage}.md")).exists());

    let journals: Vec<String> = Vec::new();
    assert_eq!(
        spool_only_stage_with_pending(&journals),
        Some(stage.to_string())
    );

    assert!(list(None, None).is_ok());
}
