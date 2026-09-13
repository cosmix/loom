//! `loom subagents` polling cases for `hooks_poll_guard.rs`.
//!
//! The parent owns the hook harness; this sibling pins the list-specific
//! escalation and proves that waiting and watching are not Bash polling.

use super::*;

#[test]
fn subagents_list_warns_twice_then_denies() {
    if skip_unless_gate_visible("subagents::subagents_list_warns_twice_then_denies") {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new().with_live_main_agent();
    session.enable_deny();

    for n in 1..=2 {
        let out = run_bash_hook(&hook, "loom subagents list --json", &session, None);
        assert_eq!(out.code, 0, "run {n}: stderr={}", out.stderr);
        assert!(out.stdout.trim().is_empty(), "run {n}: {}", out.stdout);
    }
    for n in 3..=4 {
        let out = run_bash_hook(&hook, "loom subagents list --json", &session, None);
        assert_eq!(out.code, 0, "run {n}: stderr={}", out.stderr);
        assert!(warn_context(&out.stdout).contains(&format!("run {n} times")));
    }
    let out = run_bash_hook(&hook, "loom subagents list --json", &session, None);
    assert_eq!(out.code, 2, "stderr={}", out.stderr);
    assert!(out.stderr.contains("run 5 times"), "stderr={}", out.stderr);
}

#[test]
fn subagents_wait_and_watch_do_not_count() {
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let receipt = "a".repeat(64);

    for command in [
        format!("loom subagents wait --receipt {receipt} --timeout 1"),
        "loom subagents watch --timeout 1".to_string(),
    ] {
        for n in 1..=5 {
            let out = run_bash_hook(&hook, &command, &session, None);
            assert_eq!(out.code, 0, "{command} run {n}: stderr={}", out.stderr);
            assert!(
                out.stdout.trim().is_empty(),
                "{command} run {n}: {}",
                out.stdout
            );
        }
    }
}
