//! `loom subagents` polling cases for `hooks_poll_guard.rs`.
//!
//! The parent owns the hook harness; this sibling pins the list-specific
//! escalation, the wait exemption, and the bound on one owned watch.

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
fn subagents_wait_does_not_count_and_owned_watch_repeat_denies() {
    if skip_unless_gate_visible(
        "subagents::subagents_wait_does_not_count_and_owned_watch_repeat_denies",
    ) {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new().with_live_main_agent();
    session.enable_deny();
    let receipt = "a".repeat(64);

    let wait = format!("loom subagents wait --receipt {receipt} --timeout 1");
    for n in 1..=5 {
        let out = run_bash_hook(&hook, &wait, &session, None);
        assert_eq!(out.code, 0, "{wait} run {n}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "{wait} run {n}: {}",
            out.stdout
        );
    }

    let watch = "loom subagents watch --worker claude:a1 --worker codex:u1 --timeout 1";
    let first = run_bash_hook(&hook, watch, &session, None);
    assert_eq!(first.code, 0, "first watch: stderr={}", first.stderr);
    assert!(
        first.stdout.trim().is_empty(),
        "first watch: {}",
        first.stdout
    );

    let repeated = run_bash_hook(&hook, watch, &session, None);
    assert_eq!(
        repeated.code, 2,
        "repeated watch: stderr={}",
        repeated.stderr
    );
    assert!(
        repeated.stderr.contains("AlreadyWaiting") && repeated.stderr.contains("exit 4"),
        "repeated watch omitted guidance: {}",
        repeated.stderr
    );
}
