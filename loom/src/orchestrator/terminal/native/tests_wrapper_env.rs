//! Unit tests for the wrapper script's `LOOM_SESSION_TYPE` export.
//!
//! Split out of `tests.rs` to keep it under the 400-line ceiling (CLAUDE.md
//! Rule 17), matching how `tests_capsule.rs` and `tests_launch.rs` are split
//! out of the same directory.

use super::*;
use tempfile::TempDir;

fn wrapper_script_for(kind: SessionType) -> String {
    let work_dir = TempDir::new().unwrap();
    let path = create_wrapper_script(
        work_dir.path(),
        "loom-test-session",
        "feature",
        "session1",
        "claude 'prompt'",
        None,
        kind,
        100_000,
    )
    .unwrap();
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn wrapper_script_exports_session_type() {
    let script = wrapper_script_for(SessionType::Adjudication);
    assert!(
        script.contains("LOOM_SESSION_TYPE=adjudication"),
        "{script}"
    );
    let script = wrapper_script_for(SessionType::Stage);
    assert!(script.contains("LOOM_SESSION_TYPE=stage"), "{script}");
}

#[test]
fn wrapper_script_exports_ripgrep_config_path_when_published() {
    let work_dir = TempDir::new().unwrap();
    std::fs::write(work_dir.path().join("ripgreprc"), "").unwrap();
    let path = create_wrapper_script(
        work_dir.path(),
        "loom-test-session",
        "feature",
        "session1",
        "claude 'prompt'",
        None,
        SessionType::Stage,
        100_000,
    )
    .unwrap();
    let script = std::fs::read_to_string(path).unwrap();

    let line = script
        .lines()
        .find(|line| line.contains("RIPGREP_CONFIG_PATH="))
        .unwrap_or_else(|| panic!("no RIPGREP_CONFIG_PATH line in script: {script}"));
    assert!(line.contains("/ripgreprc"), "{line}");
    assert!(script.contains("LOOM_WORK_DIR="), "{script}");
}

#[test]
fn wrapper_script_omits_ripgrep_config_path_when_unpublished() {
    let script = wrapper_script_for(SessionType::Stage);
    assert!(!script.contains("RIPGREP_CONFIG_PATH="), "{script}");
    assert!(script.contains("LOOM_WORK_DIR="), "{script}");
}

/// Pins `LOOM_SCCACHE` for a test's duration and restores it on drop. Process-
/// global, so both tests below run `#[serial]` (same convention as
/// `commands::stage::tests::state::EnvVarGuard`).
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

    /// Clears `key` for the guard's lifetime, restoring whatever value (or
    /// absence) it had on drop. Used to strip an ambient `RUSTC_WRAPPER`,
    /// `SCCACHE_DIR`, or `SCCACHE_CACHE_SIZE` a developer's own shell may
    /// have set, so the sccache tests below observe only what `LOOM_SCCACHE`
    /// controls.
    fn unset(key: &'static str) -> Self {
        let original = std::env::var(key).ok();
        std::env::remove_var(key);
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

fn fake_sccache_executable(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("sccache");
    std::fs::write(&path, "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }
    path
}

#[test]
#[serial_test::serial]
fn wrapper_script_exports_rustc_wrapper_when_sccache_is_pinned() {
    let sccache_dir = TempDir::new().unwrap();
    let fake = fake_sccache_executable(sccache_dir.path());
    let _rustc_wrapper = EnvVarGuard::unset("RUSTC_WRAPPER");
    let _sccache_dir = EnvVarGuard::unset("SCCACHE_DIR");
    let _sccache_cache_size = EnvVarGuard::unset("SCCACHE_CACHE_SIZE");
    let _pin = EnvVarGuard::set("LOOM_SCCACHE", fake.to_str().unwrap());

    let script = wrapper_script_for(SessionType::Stage);
    assert!(
        script.contains(&format!("RUSTC_WRAPPER={}", fake.display())),
        "{script}"
    );
}

#[test]
#[serial_test::serial]
fn wrapper_script_omits_rustc_wrapper_when_sccache_is_disabled() {
    let _rustc_wrapper = EnvVarGuard::unset("RUSTC_WRAPPER");
    let _sccache_dir = EnvVarGuard::unset("SCCACHE_DIR");
    let _sccache_cache_size = EnvVarGuard::unset("SCCACHE_CACHE_SIZE");
    let _pin = EnvVarGuard::set("LOOM_SCCACHE", "0");

    let script = wrapper_script_for(SessionType::Stage);
    // `RUSTC_WRAPPER` (bare, no `=`) still names the var in the
    // host-forwarding allowlist loop — only the explicit `NAME=value`
    // assignment must be absent.
    assert!(!script.contains("RUSTC_WRAPPER="), "{script}");
}
