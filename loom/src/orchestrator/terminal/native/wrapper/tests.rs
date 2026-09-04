use super::*;
use tempfile::TempDir;

/// Execs of a just-written wrapper script transiently fail with `ETXTBSY` when
/// a sibling test thread forks between the write and the spawn: the child
/// holds a duplicate of the write fd until its own exec closes it. Retry until
/// that window passes.
///
/// Sibling of `retry_past_etxtbsy` in `commands/self_update/tests.rs` - that
/// one is `anyhow::Result`-based for its callers; this one works directly on
/// the `std::io::Result` that `Command::output`/`Command::status` return.
fn retry_past_etxtbsy<T>(mut attempt: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    const ETXTBSY: i32 = 26; // "Text file busy"
    const MAX_ATTEMPTS: u32 = 50;

    let mut attempts = 0;
    loop {
        attempts += 1;
        match attempt() {
            Ok(value) => return Ok(value),
            Err(error) => {
                if error.raw_os_error() != Some(ETXTBSY) || attempts >= MAX_ATTEMPTS {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

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
        150_000,
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
        150_000,
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
        150_000,
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
        150_000,
    )
    .unwrap();

    let regular_content = fs::read_to_string(&regular_wrapper_path).unwrap();
    assert!(!regular_content.contains("LOOM_MERGE_SESSION"));
}

/// Only stage sessions own a worktree. Merge, knowledge, base-conflict and
/// adjudication sessions `cd` into the main repo, so exporting
/// `LOOM_WORKTREE_PATH` for them would make presence-based gates treat a
/// main-repo agent as a sandboxed worktree agent — which used to make knowledge
/// stages impossible to complete, and which `loom stage adjudicate` now reads
/// the other way round to refuse a verdict written from a stage worktree.
#[test]
fn worktree_path_is_exported_only_for_stage_sessions() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let repo_root = Path::new("/tmp/loom-main-repo");

    for kind in [
        SessionType::Knowledge,
        SessionType::Merge,
        SessionType::BaseConflict,
        SessionType::Adjudication,
    ] {
        let wrapper = create_wrapper_script(
            work_dir,
            &format!("loom-{kind:?}-stage-session-1"),
            "knowledge-bootstrap",
            "session-1",
            "claude 'test'",
            Some(repo_root),
            kind,
            150_000,
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
        150_000,
    )
    .unwrap();
    let stage_content = fs::read_to_string(&stage_wrapper).unwrap();
    assert!(stage_content.contains("LOOM_WORKTREE_PATH"));
    assert!(stage_content.contains("/tmp/repo/.worktrees/build-api"));
}

/// A session that dies seconds after spawn takes its terminal pane — and every
/// word claude printed there — with it. The tee is what makes the refusal
/// readable afterwards, so its exact placement is pinned: on the claude command
/// line itself (on any earlier continuation line it would attach to `env`),
/// covering stderr ONLY (stdout carries the TUI and must stay a TTY).
#[test]
fn wrapper_tees_claude_stderr_into_the_session_log() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let session_id = "session-stderr-1234567890";
    let claude_cmd = "claude 'test prompt'";

    let wrapper_path = create_wrapper_script(
        work_dir,
        "loom-test-stage-session-stderr-1234567890",
        "test-stage",
        session_id,
        claude_cmd,
        None,
        SessionType::Stage,
        150_000,
    )
    .unwrap();

    assert!(work_dir.join("logs").is_dir());

    let content = fs::read_to_string(&wrapper_path).unwrap();
    let exec_line = content
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap();

    let expected_log = fs::canonicalize(work_dir.join("logs"))
        .unwrap()
        .join(format!("{session_id}.stderr.log"));
    let expected_log = escape(expected_log.display().to_string().into());

    assert!(exec_line.contains(claude_cmd), "{exec_line}");
    assert!(
        exec_line.ends_with(&format!(
            "2> >(env -i \"${{_loom_env[@]}}\" tee -a {expected_log})"
        )),
        "{exec_line}"
    );
    // stdout is left alone: redirecting it would cost the session its TTY.
    assert!(!exec_line.contains("1>"), "{exec_line}");
}

/// The assertion above pins the text of the redirection; this one pins its
/// behaviour by running the wrapper and reading the log back. A malformed
/// process substitution would still satisfy a string match while breaking the
/// script for every session loom spawns.
///
/// Note where the captured text surfaces: `tee` inherits the wrapper's fd 1, so
/// the message reaches the log AND the process's stdout. In a terminal both
/// streams are the same pane, which is the point — the operator still sees the
/// refusal live, and loom still has it on disk afterwards.
#[test]
fn wrapper_stderr_capture_survives_execution() {
    let temp_dir = TempDir::new().unwrap();
    let work_dir = temp_dir.path();
    let session_id = "session-exec-stderr-1";

    let wrapper_path = create_wrapper_script(
        work_dir,
        "loom-exec-stage-session-exec-stderr-1",
        "exec-stage",
        session_id,
        "sh -c 'echo refusal-canary >&2; echo tui-canary'",
        None,
        SessionType::Stage,
        150_000,
    )
    .unwrap();

    // `output()` reads the pipes to EOF before returning, and `tee` holds fd 1
    // open until it has flushed, so the log is complete by the time this call
    // comes back.
    let output = retry_past_etxtbsy(|| std::process::Command::new(&wrapper_path).output()).unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tui-canary"), "{stdout}");
    assert!(stdout.contains("refusal-canary"), "{stdout}");

    let log = fs::read_to_string(stderr_log_path(work_dir, session_id)).unwrap();
    assert!(log.contains("refusal-canary"), "{log}");
    // Only stderr is teed — the TUI's own stream never reaches the log.
    assert!(!log.contains("tui-canary"), "{log}");
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
        150_000,
    )
    .unwrap();

    let status = retry_past_etxtbsy(|| {
        std::process::Command::new(&wrapper)
            .env("ANTHROPIC_API_KEY", "ambient-secret-canary")
            .env("GITHUB_TOKEN", "ambient-secret-canary")
            .status()
    })
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
