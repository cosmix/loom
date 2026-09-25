//! Regression guard for the fake-HOME test isolation the rest of the suite
//! relies on: `loom repair --fix` drives the same bare `install_loom_hooks()`
//! / `install_codex_hooks()` writers that `loom init` and unit tests exercise
//! (see `commands/repair/hooks.rs::install_hook_assets`), both of which
//! resolve their target through `dirs::home_dir()` rather than an injectable
//! parameter. This proves that redirecting `HOME` before spawning the binary
//! — the same mechanism `helpers::loom_cmd()` callers and `HomeGuard`-style
//! unit tests use — actually keeps that write off the real developer HOME
//! and lands it in the redirected one instead.

use std::fs;

use super::helpers::{init_test_repo, loom_cmd};

#[test]
fn repair_fix_installs_hooks_into_the_redirected_home_only() {
    let repo = init_test_repo();
    let fake_home = tempfile::TempDir::new().unwrap();

    let output = loom_cmd()
        .env("HOME", fake_home.path())
        .current_dir(repo.path())
        .arg("repair")
        .arg("--fix")
        .output()
        .expect("failed to run loom repair --fix");

    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let claude_hook = fake_home.path().join(".claude/hooks/loom/commit-guard.sh");
    assert!(
        claude_hook.is_file(),
        "expected loom hooks installed under the redirected HOME at {}",
        claude_hook.display()
    );
    let codex_hook = fake_home.path().join(".codex/hooks/loom/commit-guard.sh");
    assert!(
        codex_hook.is_file(),
        "expected codex-native hooks installed under the redirected HOME at {}",
        codex_hook.display()
    );

    // Real content, not an empty stub: the same marker `install_tests.rs`
    // checks for the `_to` variant of this writer.
    let content = fs::read_to_string(&claude_hook).unwrap();
    assert!(content.contains("detect_loom_worktree"));
}
