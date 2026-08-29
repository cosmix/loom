//! Verify-runner exemption, the `echo`-is-never-counted case, and the live-session deny gate
//! regression for `hooks_poll_guard.rs`.
//!
//! All three exercise rule 2 (the repeated read-only poll counter) and are grouped here purely
//! for size - sharing the parent's harness (hook installation, `Session`, `run_bash_hook`,
//! `warn_context`) via `use super::*` - read the parent's module docs first.

use super::*;

// 2. Build/test/lint runners are exempt from the repeat-command rule
//    outright - the acceptance loop is SUPPOSED to rerun them. Never denied,
//    never warned, not even on the 5th run, not even with the switch on and
//    a live main-agent process above the hook.
#[test]
fn verify_runners_are_never_denied_or_warned_on_repeat() {
    let (_hook_dir, hook) = setup_hook();
    for command in ["cargo test", "cargo clippy -- -D warnings", "npm test"] {
        let session = Session::new().with_live_main_agent();
        session.enable_deny();
        for n in 1..=5 {
            let out = run_bash_hook(&hook, command, &session, None);
            assert_eq!(out.code, 0, "{command} run {n}: stderr={}", out.stderr);
            assert!(
                out.stdout.trim().is_empty(),
                "{command} run {n} must be silent - verify runners are exempt from the repeat rule: {}",
                out.stdout
            );
        }
    }
}

// New. `echo` was removed from the countable read-only poll list - it is as
// often a write verb (`echo ... >> file`) as a read one. Five repeats never
// warn or deny, even with the switch on and a live main-agent process.
#[test]
fn echo_is_never_counted_toward_the_repeat_rule() {
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new().with_live_main_agent();
    session.enable_deny();

    for n in 1..=5 {
        let out = run_bash_hook(&hook, "echo something >> notes.md", &session, None);
        assert_eq!(out.code, 0, "run {n}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "run {n} must be silent - echo is not a countable poll: {}",
            out.stdout
        );
    }
}

// New (BLOCKER fix regression). Same regression as read-guard.sh's own gate
// test: a deny fires ONLY when BOTH the `[hooks] deny_enabled` switch is on
// AND `LOOM_MAIN_AGENT_PID` is set to a LIVE ancestor of the hook's bash
// process. The switch alone, and the switch plus a PID that is NOT a live
// ancestor (e.g. "1"), must both still only warn at the 5th repeated
// `git status` - never deny.
#[test]
fn deny_requires_a_live_main_agent_not_just_the_switch() {
    let (_hook_dir, hook) = setup_hook();

    let switch_only = Session::new();
    switch_only.enable_deny();
    for _ in 1..=4 {
        run_bash_hook(&hook, "git status", &switch_only, None);
    }
    let out = run_bash_hook(&hook, "git status", &switch_only, None);
    assert_eq!(
        out.code, 0,
        "switch alone must never deny: stderr={}",
        out.stderr
    );
    assert!(
        warn_context(&out.stdout).contains("run 5 times"),
        "stdout={}",
        out.stdout
    );

    let non_ancestor = Session::new().with_main_agent_pid("1");
    non_ancestor.enable_deny();
    for _ in 1..=4 {
        run_bash_hook(&hook, "git status", &non_ancestor, None);
    }
    let out = run_bash_hook(&hook, "git status", &non_ancestor, None);
    assert_eq!(
        out.code, 0,
        "a LOOM_MAIN_AGENT_PID that is not a live ancestor must never deny: stderr={}",
        out.stderr
    );
    assert!(
        warn_context(&out.stdout).contains("run 5 times"),
        "stdout={}",
        out.stdout
    );
}
