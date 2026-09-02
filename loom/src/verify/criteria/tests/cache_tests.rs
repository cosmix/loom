//! Tests for the acceptance-criterion pass cache

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serial_test::serial;
use tempfile::TempDir;

use crate::verify::criteria::cache::{
    compute_cache_key, is_cacheable, lookup_pass, store_pass, CachePolicy, CachedPass,
};

const TEST_IDENTITY: &[&str] = &["-c", "user.name=Test", "-c", "user.email=test@example.com"];

fn git(args: &[&str], dir: &Path) {
    let mut full = TEST_IDENTITY.to_vec();
    full.extend_from_slice(args);
    let status = Command::new("git")
        .args(&full)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {}", dir.display());
}

fn init_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    git(&["init", "-q"], temp.path());
    std::fs::write(temp.path().join("file.txt"), "hello\n").unwrap();
    git(&["add", "file.txt"], temp.path());
    git(&["commit", "-q", "-m", "init"], temp.path());
    temp
}

/// Pins `LOOM_ACCEPTANCE_CACHE` for a test's duration and restores it on drop.
struct EnvVarGuard {
    key: &'static str,
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let original = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

#[test]
fn key_changes_with_command_text() {
    let repo = init_repo();
    let a = compute_cache_key("echo a", repo.path()).unwrap();
    let b = compute_cache_key("echo b", repo.path()).unwrap();
    assert_ne!(a.digest, b.digest);
}

#[test]
fn key_changes_with_head() {
    let repo = init_repo();
    let before = compute_cache_key("echo hi", repo.path()).unwrap();

    std::fs::write(repo.path().join("file.txt"), "hello\nmore\n").unwrap();
    git(&["add", "file.txt"], repo.path());
    git(&["commit", "-q", "-m", "second"], repo.path());

    let after = compute_cache_key("echo hi", repo.path()).unwrap();
    assert_ne!(before.digest, after.digest);
    assert_ne!(before.tree_head, after.tree_head);
}

#[test]
fn key_changes_with_tracked_modification() {
    let repo = init_repo();
    let before = compute_cache_key("echo hi", repo.path()).unwrap();

    std::fs::write(repo.path().join("file.txt"), "hello\nchanged\n").unwrap();

    let after = compute_cache_key("echo hi", repo.path()).unwrap();
    assert_ne!(before.digest, after.digest);
}

#[test]
fn key_changes_with_new_untracked_file() {
    let repo = init_repo();
    let before = compute_cache_key("echo hi", repo.path()).unwrap();

    std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();

    let after = compute_cache_key("echo hi", repo.path()).unwrap();
    assert_ne!(before.digest, after.digest);
}

#[test]
fn identical_trees_give_identical_keys() {
    let repo = init_repo();
    let a = compute_cache_key("echo hi", repo.path()).unwrap();
    let b = compute_cache_key("echo hi", repo.path()).unwrap();
    assert_eq!(a.digest, b.digest);
}

#[test]
fn non_git_directory_yields_no_key() {
    let temp = TempDir::new().unwrap();
    assert!(compute_cache_key("echo hi", temp.path()).is_none());
}

#[test]
fn stored_pass_is_found_by_lookup() {
    let repo = init_repo();
    let work_dir = TempDir::new().unwrap();
    let key = compute_cache_key("echo hi", repo.path()).unwrap();

    let record = CachedPass::from_result("echo hi", repo.path(), &key.tree_head, "out", "", 5);
    store_pass(work_dir.path(), &key.digest, &record).unwrap();

    let found = lookup_pass(work_dir.path(), &key.digest).unwrap();
    assert_eq!(found.command, "echo hi");
    assert_eq!(found.exit_code, 0);
    assert_eq!(found.tree_head, key.tree_head);
}

#[test]
fn lookup_misses_when_nothing_stored() {
    let work_dir = TempDir::new().unwrap();
    assert!(lookup_pass(work_dir.path(), "nonexistent-key").is_none());
}

#[test]
fn cached_pass_truncates_output_to_tail() {
    let repo = init_repo();
    let huge = "x".repeat(10 * 1024);
    let record = CachedPass::from_result("echo hi", repo.path(), "deadbeef", &huge, &huge, 1);
    assert_eq!(record.stdout_tail.len(), 4 * 1024);
    assert_eq!(record.stderr_tail.len(), 4 * 1024);
    let _ = Duration::from_millis(record.duration_ms);
}

#[test]
#[serial]
fn cache_policy_bypass_from_env() {
    let _guard = EnvVarGuard::set("LOOM_ACCEPTANCE_CACHE", "0");
    assert_eq!(CachePolicy::from_env(), CachePolicy::Bypass);
}

#[test]
#[serial]
fn cache_policy_use_when_env_other_value() {
    let _guard = EnvVarGuard::set("LOOM_ACCEPTANCE_CACHE", "1");
    assert_eq!(CachePolicy::from_env(), CachePolicy::Use);
}

#[test]
fn is_cacheable_rejects_home_reference() {
    let repo = init_repo();
    assert!(!is_cacheable("echo $HOME/foo", repo.path()));
}

#[test]
fn is_cacheable_rejects_tilde_path() {
    let repo = init_repo();
    assert!(!is_cacheable("cat ~/secrets", repo.path()));
}

#[test]
fn is_cacheable_rejects_mktemp() {
    let repo = init_repo();
    assert!(!is_cacheable("d=$(mktemp -d)", repo.path()));
}

#[test]
fn is_cacheable_rejects_loom_home() {
    let repo = init_repo();
    assert!(!is_cacheable("echo $LOOM_HOME", repo.path()));
}

#[test]
fn is_cacheable_accepts_ordinary_command() {
    let repo = init_repo();
    assert!(is_cacheable("cargo test", repo.path()));
}

#[test]
fn is_cacheable_rejects_braced_home_reference() {
    let repo = init_repo();
    assert!(!is_cacheable("echo ${HOME}/x", repo.path()));
}

#[test]
fn is_cacheable_rejects_braced_home_with_default() {
    let repo = init_repo();
    assert!(!is_cacheable("echo ${HOME:-/tmp}", repo.path()));
}

#[test]
fn is_cacheable_rejects_user_variable() {
    let repo = init_repo();
    assert!(!is_cacheable("echo $USER", repo.path()));
}

#[test]
fn is_cacheable_accepts_similarly_prefixed_variable() {
    let repo = init_repo();
    assert!(is_cacheable("echo $HOMEBREW_PREFIX/bin", repo.path()));
}

#[test]
fn is_cacheable_rejects_command_naming_ignored_path() {
    let repo = init_repo();
    std::fs::write(repo.path().join(".gitignore"), "target/\n").unwrap();

    assert!(!is_cacheable("./target/debug/loom --version", repo.path()));
    assert!(is_cacheable("cargo test --lib", repo.path()));
    assert!(is_cacheable("cat src/main.rs", repo.path()));
}

#[test]
fn is_cacheable_ignores_dollar_prefixed_token() {
    let repo = init_repo();
    assert!(is_cacheable("echo $H/.loom/x", repo.path()));
}

#[test]
fn is_cacheable_rejects_path_token_outside_git_repo() {
    let temp = TempDir::new().unwrap();
    assert!(!is_cacheable("./target/debug/loom --version", temp.path()));
}

#[test]
fn key_changes_when_large_file_size_changes() {
    let repo = init_repo();
    let nine_mib = vec![0u8; 9 * 1024 * 1024];
    std::fs::write(repo.path().join("big.bin"), &nine_mib).unwrap();
    let before = compute_cache_key("echo hi", repo.path()).unwrap();

    let mut grown = nine_mib;
    grown.extend_from_slice(&[1u8; 1024]);
    std::fs::write(repo.path().join("big.bin"), &grown).unwrap();
    let after = compute_cache_key("echo hi", repo.path()).unwrap();

    assert_ne!(before.digest, after.digest);
}
