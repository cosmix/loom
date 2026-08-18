use super::*;
use tempfile::TempDir;

#[test]
fn test_wrapper_script_creation() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let stage_id = "test-stage";
    let session_id = "session-abc123-1234567890";
    let pid_key = "loom-test-stage-session-abc123-1234567890";
    let claude_cmd = "claude 'test prompt'";

    let wrapper_path = create_wrapper_script(
        work_dir,
        pid_key,
        stage_id,
        session_id,
        claude_cmd,
        None,
        SessionType::Stage,
    )
    .unwrap();

    assert!(wrapper_path.exists());
    let content = fs::read_to_string(&wrapper_path).unwrap();
    assert!(content.contains("#!/bin/bash"));
    assert!(content.contains("echo $$"));
    assert!(content.contains(claude_cmd));
    assert!(content.contains("LOOM_SESSION_ID"));
    assert!(content.contains(session_id));
    assert!(content.contains("LOOM_STAGE_ID"));
    assert!(content.contains(stage_id));
    assert!(content.contains("LOOM_WORK_DIR"));
    assert!(content.contains("LOOM_MAIN_AGENT_PID"));
    assert!(content.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"));
    assert!(content.contains("CLAUDE_REMOTE_CONTROL_SESSION_NAME_PREFIX=loom"));
    // See the ENV_ALLOWLIST doc comment above: TERM without its terminfo
    // location is half a contract.
    assert!(content.contains("TERMINFO"));
    assert!(content.contains("TERMINFO_DIRS"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::metadata(&wrapper_path).unwrap().permissions();
        assert!(perms.mode() & 0o111 != 0);
    }
}

#[test]
fn test_wrapper_script_with_working_dir() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let stage_id = "test-stage-cwd";
    let session_id = "session-def456-9876543210";
    let pid_key = "loom-test-stage-cwd-session-def456-9876543210";
    let claude_cmd = "claude 'test prompt'";
    let working_dir = Path::new("/tmp/test-worktree");

    let wrapper_path = create_wrapper_script(
        work_dir,
        pid_key,
        stage_id,
        session_id,
        claude_cmd,
        Some(working_dir),
        SessionType::Stage,
    )
    .unwrap();

    assert!(wrapper_path.exists());
    let content = fs::read_to_string(&wrapper_path).unwrap();
    assert!(content.contains("#!/bin/bash"));
    assert!(content.contains("cd /tmp/test-worktree"));
    assert!(content.contains("echo $$"));
    assert!(content.contains(claude_cmd));
    assert!(content.contains("LOOM_WORKTREE_PATH"));
    assert!(content.contains("/tmp/test-worktree"));
    assert!(content.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"));
}

/// `LOOM_MERGE_SESSION` follows the session KIND, not a `merge-` prefix on the
/// stage id. The stage id handed to the wrapper is the plain plan stage id for
/// every kind, so a prefix sniff would both miss real merge sessions and fire
/// on a plan stage that merely happens to be named `merge-something`.
#[test]
fn merge_session_env_follows_kind_not_stage_id_prefix() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let claude_cmd = "claude 'resolve merge conflict'";

    // A merge session for a stage whose id carries no prefix at all.
    let wrapper_path = create_wrapper_script(
        work_dir,
        "loom-merge-test-stage-session-merge-1234567890",
        "test-stage",
        "session-merge-1234567890",
        claude_cmd,
        None,
        SessionType::Merge,
    )
    .unwrap();

    let content = fs::read_to_string(&wrapper_path).unwrap();
    assert!(content.contains("LOOM_MERGE_SESSION=1"));
    assert!(content.contains(claude_cmd));

    // A REGULAR stage session whose stage id starts with `merge-` must not be
    // mistaken for one.
    let regular_wrapper_path = create_wrapper_script(
        work_dir,
        "loom-merge-adjacent-stage-session-regular-1234567890",
        "merge-adjacent-stage",
        "session-regular-1234567890",
        claude_cmd,
        None,
        SessionType::Stage,
    )
    .unwrap();

    let regular_content = fs::read_to_string(&regular_wrapper_path).unwrap();
    assert!(!regular_content.contains("LOOM_MERGE_SESSION"));
}

/// Only stage sessions own a worktree. Merge, knowledge and base-conflict
/// sessions `cd` into the main repo, so exporting `LOOM_WORKTREE_PATH` for them
/// would make presence-based gates treat a main-repo agent as a sandboxed
/// worktree agent — which used to make knowledge stages impossible to complete.
#[test]
fn worktree_path_is_exported_only_for_stage_sessions() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let repo_root = Path::new("/tmp/loom-main-repo");

    for kind in [
        SessionType::Knowledge,
        SessionType::Merge,
        SessionType::BaseConflict,
    ] {
        let wrapper = create_wrapper_script(
            work_dir,
            &format!("loom-{kind:?}-stage-session-1"),
            "knowledge-bootstrap",
            "session-1",
            "claude 'test'",
            Some(repo_root),
            kind,
        )
        .unwrap();
        let content = fs::read_to_string(&wrapper).unwrap();
        assert!(
            !content.contains("LOOM_WORKTREE_PATH"),
            "{kind:?} session must not export LOOM_WORKTREE_PATH"
        );
        // The cd target is unaffected — only the export is gated.
        assert!(content.contains("cd /tmp/loom-main-repo"));
    }

    let stage_wrapper = create_wrapper_script(
        work_dir,
        "loom-build-api-session-2",
        "build-api",
        "session-2",
        "claude 'test'",
        Some(Path::new("/tmp/repo/.worktrees/build-api")),
        SessionType::Stage,
    )
    .unwrap();
    let stage_content = fs::read_to_string(&stage_wrapper).unwrap();
    assert!(stage_content.contains("LOOM_WORKTREE_PATH"));
    assert!(stage_content.contains("/tmp/repo/.worktrees/build-api"));
}

#[test]
fn wrapper_executes_with_minimal_environment_and_no_ambient_secret() {
    let temp_dir = TempDir::new().unwrap();
    let output_path = temp_dir.path().join("stage-environment.txt");
    let output_arg = escape(output_path.display().to_string().into());
    let command = format!("env > {output_arg}");
    let wrapper = create_wrapper_script(
        temp_dir.path(),
        "loom-env-stage-session-1",
        "env-stage",
        "session-env-1",
        &command,
        None,
        SessionType::Stage,
    )
    .unwrap();

    let status = std::process::Command::new(&wrapper)
        .env("ANTHROPIC_API_KEY", "ambient-secret-canary")
        .env("GITHUB_TOKEN", "ambient-secret-canary")
        .status()
        .unwrap();
    assert!(status.success());

    let environment = fs::read_to_string(output_path).unwrap();
    assert!(environment.contains("LOOM_SESSION_ID=session-env-1"));
    assert!(environment.contains("LOOM_STAGE_ID=env-stage"));
    assert!(environment.contains("HOME="));
    assert!(environment.contains("PATH="));
    assert!(!environment.contains("ambient-secret-canary"));
    assert!(!environment.contains("ANTHROPIC_API_KEY"));
    assert!(!environment.contains("GITHUB_TOKEN"));
}
