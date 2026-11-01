use super::*;

#[test]
fn long_sleep_warns_short_sleep_does_not() {
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new();

    let out = run_bash_hook(&hook, "sleep 300", &session, None);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        warn_context(&out.stdout).contains("loom subagents watch"),
        "stdout={}",
        out.stdout
    );

    let out = run_bash_hook(&hook, "sleep 5", &session, None);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);

    let out = run_bash_hook(&hook, "sleep 29", &session, None);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty(),
        "sleep 29 must not warn: {}",
        out.stdout
    );

    let out = run_bash_hook(&hook, "sleep 30", &session, None);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        !out.stdout.trim().is_empty(),
        "sleep 30 must warn: {}",
        out.stdout
    );
}

#[test]
fn repeated_git_status_escalates_to_deny_only_with_switch_on() {
    if skip_unless_gate_visible("cases::repeated_git_status_escalates_to_deny_only_with_switch_on")
    {
        return;
    }
    let (_hook_dir, hook) = setup_hook();
    let on = Session::new().with_live_main_agent();
    on.enable_deny();
    for n in 1..=2 {
        let out = run_bash_hook(&hook, "git status", &on, None);
        assert_eq!(out.code, 0, "run {n}: stderr={}", out.stderr);
        assert!(
            out.stdout.trim().is_empty(),
            "run {n} must be clean: {}",
            out.stdout
        );
    }
    for n in 3..=4 {
        let out = run_bash_hook(&hook, "git status", &on, None);
        assert_eq!(out.code, 0, "run {n}: stderr={}", out.stderr);
        assert!(
            warn_context(&out.stdout).contains(&format!("run {n} times")),
            "run {n}: {}",
            out.stdout
        );
    }
    let out = run_bash_hook(&hook, "git status", &on, None);
    assert_eq!(
        out.code, 2,
        "5th run must deny with the switch on: stderr={}",
        out.stderr
    );
    assert!(out.stderr.contains("run 5 times"), "stderr={}", out.stderr);

    assert_repeat_poll_warns_without_deny(&hook);
}

#[test]
fn non_bash_tool_and_unmatched_command_are_silently_ignored() {
    let (_hook_dir, hook) = setup_hook();
    let session = Session::new();

    let wrong_tool = json!({
        "tool_name": "Read",
        "tool_input": {"file_path": "/tmp/whatever"},
        "agent_id": session.agent_id,
    });
    let out = run_payload(&hook, &wrong_tool, &session, None);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(
        out.stdout.trim().is_empty() && out.stderr.trim().is_empty(),
        "stdout={} stderr={}",
        out.stdout,
        out.stderr
    );

    let out = run_bash_hook(&hook, "rg -n foo src/", &session, None);
    assert_eq!(out.code, 0, "stderr={}", out.stderr);
    assert!(out.stdout.trim().is_empty(), "stdout={}", out.stdout);
}

fn assert_repeat_poll_warns_without_deny(hook: &Path) {
    let off = Session::new();
    for _ in 1..=4 {
        run_bash_hook(hook, "git status", &off, None);
    }
    let out = run_bash_hook(hook, "git status", &off, None);
    assert_eq!(
        out.code, 0,
        "switch off must never deny: stderr={}",
        out.stderr
    );
    assert!(
        warn_context(&out.stdout).contains("run 5 times"),
        "stdout={}",
        out.stdout
    );
}
