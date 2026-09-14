//! Adversarial regression tests for security-sensitive hook policy.

use crate::fs::permissions::constants::{
    HOOK_CODEX_FORWARD_COMMON, HOOK_CODEX_FORWARD_GUARD, HOOK_COMMON, HOOK_LIFECYCLE,
    HOOK_POST_TOOL_HEARTBEAT, HOOK_POST_TOOL_USE, HOOK_PROGRESS_CLASSIFICATION, HOOK_READ_LEDGER,
    HOOK_WORKTREE_FILE_GUARD,
};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

struct HookFixture {
    _temp: TempDir,
    hooks: PathBuf,
    worktree: PathBuf,
    sibling: PathBuf,
    outside: PathBuf,
    home: PathBuf,
    work_dir: PathBuf,
}

impl HookFixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        let hooks = temp.path().join("hooks");
        let worktree = repo.join(".worktrees/stage");
        let sibling = repo.join(".worktrees/stage-sibling");
        let outside = temp.path().join("outside.txt");
        let home = temp.path().join("home");
        let work_dir = repo.join(".loom").join("work");

        for path in [&hooks, &worktree, &sibling, &home, &work_dir] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(hooks.join("_common.sh"), HOOK_COMMON).unwrap();
        fs::write(hooks.join("_lifecycle.sh"), HOOK_LIFECYCLE).unwrap();
        fs::write(hooks.join("_codex_forward.sh"), HOOK_CODEX_FORWARD_COMMON).unwrap();
        fs::write(
            hooks.join("_progress-classification.sh"),
            HOOK_PROGRESS_CLASSIFICATION,
        )
        .unwrap();
        fs::write(
            hooks.join("_post-tool-heartbeat.sh"),
            HOOK_POST_TOOL_HEARTBEAT,
        )
        .unwrap();
        fs::write(hooks.join("_read_ledger.sh"), HOOK_READ_LEDGER).unwrap();
        fs::write(&outside, "outside").unwrap();
        fs::write(sibling.join("file.txt"), "sibling").unwrap();
        fs::create_dir_all(worktree.join(".loom")).unwrap();
        fs::create_dir(worktree.join(".git")).unwrap();
        symlink("../../../.loom/work", worktree.join(".loom").join("work")).unwrap();
        Self {
            _temp: temp,
            hooks,
            worktree,
            sibling,
            outside,
            home,
            work_dir,
        }
    }

    fn install(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.hooks.join(name);
        fs::write(&path, contents).unwrap();
        path
    }
}

fn run_hook(script: &Path, cwd: &Path, envs: &[(&str, &Path)], payload: &Value) -> Output {
    let mut child = Command::new("bash")
        .arg(script)
        .current_dir(cwd)
        .envs(envs.iter().map(|(key, value)| (*key, value.as_os_str())))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn file_call(fixture: &HookFixture, tool: &str, path: &str) -> Output {
    let script = fixture.install("worktree-file-guard.sh", HOOK_WORKTREE_FILE_GUARD);
    let key = if matches!(tool, "Glob" | "Grep") {
        "path"
    } else {
        "file_path"
    };
    run_hook(
        &script,
        &fixture.worktree,
        &[("HOME", &fixture.home)],
        &json!({"tool_name": tool, "tool_input": {(key): path}}),
    )
}

#[test]
fn file_guard_rejects_absolute_host_paths_and_credentials() {
    let fixture = HookFixture::new();
    assert_eq!(
        file_call(&fixture, "Write", fixture.outside.to_str().unwrap())
            .status
            .code(),
        Some(2)
    );

    let credentials = fixture.home.join(".claude/.credentials.json");
    fs::create_dir_all(credentials.parent().unwrap()).unwrap();
    fs::write(credentials, "secret").unwrap();
    assert_eq!(
        file_call(&fixture, "Read", "~/.claude/.credentials.json")
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn file_guard_rejects_symlink_leaf_and_prefix_sibling() {
    let fixture = HookFixture::new();
    let link = fixture.worktree.join("escape-link");
    symlink(&fixture.outside, &link).unwrap();

    assert_eq!(
        file_call(&fixture, "Read", link.to_str().unwrap())
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        file_call(
            &fixture,
            "Edit",
            fixture.sibling.join("file.txt").to_str().unwrap(),
        )
        .status
        .code(),
        Some(2)
    );
}

#[test]
fn file_guard_allows_normal_worktree_file_but_denies_capability_tokens() {
    let fixture = HookFixture::new();
    fs::write(fixture.worktree.join("inside.txt"), "inside").unwrap();
    fs::write(fixture.work_dir.join("admin.token"), "token").unwrap();

    assert!(file_call(&fixture, "Read", "inside.txt").status.success());
    assert_eq!(
        file_call(&fixture, "Read", ".loom/work/admin.token")
            .status
            .code(),
        Some(2)
    );
    assert!(file_call(&fixture, "Write", ".loom/work/handoffs/state.md")
        .status
        .success());
    for protected in [
        ".loom/work/memory/forged.md",
        ".loom/work/disputes/stage/1/verdict.md",
    ] {
        assert_eq!(
            file_call(&fixture, "Write", protected).status.code(),
            Some(2),
            "direct write unexpectedly authorized for {protected}"
        );
    }
}

fn forward_call(fixture: &HookFixture, payload: Value) -> Output {
    let script = fixture.install("codex-forward-guard.sh", HOOK_CODEX_FORWARD_GUARD);
    run_hook(
        &script,
        &fixture.worktree,
        &[
            ("HOME", fixture.home.as_path()),
            ("LOOM_STAGE_ID", Path::new("")),
            ("LOOM_SESSION_ID", Path::new("")),
            ("LOOM_WORK_DIR", Path::new("")),
        ],
        &payload,
    )
}

fn forward_call_in_stage(fixture: &HookFixture, payload: Value) -> Output {
    let script = fixture.install("codex-forward-guard.sh", HOOK_CODEX_FORWARD_GUARD);
    run_hook(
        &script,
        &fixture.worktree,
        &[
            ("HOME", fixture.home.as_path()),
            ("LOOM_STAGE_ID", Path::new("policy-stage")),
            ("LOOM_SESSION_ID", Path::new("loom-session")),
            ("LOOM_WORK_DIR", fixture.work_dir.as_path()),
        ],
        &payload,
    )
}

#[test]
fn forward_guard_allows_only_exact_forward_wrapper_command() {
    let fixture = HookFixture::new();
    let command = "~/.claude/hooks/loom/codex-forward.sh task 'hello; literal' --model gpt-5.6-terra --effort xhigh --write --unit-id policy-unit";
    install_forward_companion(&fixture);

    let invocation = assert_forward_allowed(&fixture, command);
    assert_authorization_row(&fixture, &invocation);
    assert_outside_stage_rejected(&fixture, command);
    assert_missing_companion_rejected(command);
}

fn install_forward_companion(fixture: &HookFixture) {
    let companion = fixture
        .home
        .join(".claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs");
    fs::create_dir_all(companion.parent().unwrap()).unwrap();
    fs::write(companion, "// pinned fixture\n").unwrap();
    install_forward_start(fixture);
}

fn install_forward_start(fixture: &HookFixture) {
    let directory = fixture.work_dir.join("subagents/policy-stage");
    fs::create_dir_all(&directory).unwrap();
    let row = json!({"agent_id":"policy-forwarder","agent_type":"loom-codex-forwarder",
        "stage_id":"policy-stage","loom_session_id":"loom-session",
        "parent_session_id":"parent-session","ts":"2000-01-01T00:00:00.000Z"});
    fs::write(directory.join("starts.jsonl"), format!("{row}\n")).unwrap();
}

fn forward_payload(fixture: &HookFixture, command: &str, tool_use_id: &str) -> Value {
    json!({
        "tool_name": "Bash",
        "agent_type": "loom-codex-forwarder",
        "agent_id": "policy-forwarder",
        "session_id": "parent-session",
        "tool_use_id": tool_use_id,
        "cwd": fixture.worktree,
        "tool_input": {"command": command, "timeout": 600000}
    })
}

fn assert_forward_allowed(fixture: &HookFixture, command: &str) -> String {
    let allowed = forward_call_in_stage(fixture, forward_payload(fixture, command, "policy-tool"));
    assert!(allowed.status.success(), "{:?}", allowed.stderr);
    let response: Value = serde_json::from_slice(&allowed.stdout).unwrap();
    let updated = response["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str()
        .unwrap();
    let prefix = format!("{command} --invocation-id ");
    let invocation = updated.strip_prefix(&prefix).unwrap();
    let nonce = invocation.strip_prefix("inv-").unwrap();
    assert_eq!(nonce.len(), 32);
    assert!(nonce
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    assert_eq!(
        response["hookSpecificOutput"],
        json!({
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": {"command": updated, "timeout": 600000}
        })
    );
    invocation.to_owned()
}

fn assert_authorization_row(fixture: &HookFixture, invocation: &str) {
    let ledger =
        fs::read_to_string(fixture.work_dir.join("subagents/policy-stage/codex.jsonl")).unwrap();
    assert_eq!(ledger.lines().count(), 1);
    let row: Value = serde_json::from_str(ledger.trim()).unwrap();
    assert_eq!(row["v"], 2);
    assert_eq!(row["stage_id"], "policy-stage");
    assert_eq!(row["session_id"], "loom-session");
    assert_eq!(row["parent_session_id"], "parent-session");
    assert_eq!(row["forwarder_agent_id"], "policy-forwarder");
    assert_eq!(row["tool_use_id"], "policy-tool");
    assert_eq!(row["unit_id"], "policy-unit");
    assert_eq!(row["invocation_id"], invocation);
    assert_eq!(row["model"], "gpt-5.6-terra");
    assert_eq!(row["effort"], "xhigh");
}

fn assert_outside_stage_rejected(fixture: &HookFixture, command: &str) {
    let outside = forward_call(fixture, forward_payload(fixture, command, "outside-tool"));
    assert_eq!(outside.status.code(), Some(2));
    assert!(outside.stdout.is_empty());
    assert!(String::from_utf8_lossy(&outside.stderr).contains(
        "codex forwarding is allowed only inside an active loom stage (safe LOOM_STAGE_ID, LOOM_SESSION_ID, and LOOM_WORK_DIR are required)"
    ));
}

fn assert_missing_companion_rejected(command: &str) {
    let missing = HookFixture::new();
    install_forward_start(&missing);
    let no_companion =
        forward_call_in_stage(&missing, forward_payload(&missing, command, "missing-tool"));
    assert_eq!(no_companion.status.code(), Some(2));
    assert!(no_companion.stdout.is_empty());
    assert!(String::from_utf8_lossy(&no_companion.stderr)
        .contains("supported codex companion 1.0.6 is missing or unsafe"));
}

#[test]
fn forward_guard_rejects_shell_operators_substrings_and_missing_metadata() {
    let fixture = HookFixture::new();
    let base = "~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write";
    for command in [
        format!("{base}; touch escaped"),
        format!("{base} | sh"),
        format!("{base}\t--background"),
        "/tmp/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write".to_string(),
        "printf codex-forward.sh".to_string(),
    ] {
        let payload = json!({
            "tool_name": "Bash",
            "agent_type": "loom-codex-forwarder",
            "tool_input": {"command": command}
        });
        assert_eq!(forward_call(&fixture, payload).status.code(), Some(2));
    }

    let missing = json!({"tool_name": "Read", "tool_input": {"file_path": "README.md"}});
    assert_eq!(forward_call(&fixture, missing).status.code(), Some(2));
}

#[test]
fn post_tool_hook_persists_only_a_private_heartbeat() {
    let fixture = HookFixture::new();
    let script = fixture.install("post-tool-use.sh", HOOK_POST_TOOL_USE);
    let payload = json!({
        "tool_name": "Bash",
        "tool_input": {"command": "true"},
        "tool_result": {"output": "TOP-SECRET-VALUE", "is_error": false, "exit_code": 0}
    });
    let envs = [
        ("LOOM_STAGE_ID", Path::new("stage")),
        ("LOOM_SESSION_ID", Path::new("session")),
        ("LOOM_WORK_DIR", fixture.work_dir.as_path()),
    ];
    assert!(run_hook(&script, &fixture.worktree, &envs, &payload)
        .status
        .success());

    let heartbeat = fixture.work_dir.join("heartbeat/stage.json");
    let content = fs::read_to_string(&heartbeat).unwrap();
    assert!(!content.contains("TOP-SECRET-VALUE"));
    let heartbeat_json: Value = serde_json::from_str(&content).unwrap();
    assert_eq!(heartbeat_json["activity_kind"], "progress");
    assert_eq!(heartbeat_json["progress_at"], heartbeat_json["timestamp"]);
    assert_eq!(
        fs::metadata(&heartbeat).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!fixture.work_dir.join("tool-events.jsonl").exists());

    let victim = fixture._temp.path().join("victim");
    fs::write(&victim, "unchanged").unwrap();
    let events = fixture.work_dir.join("tool-events.jsonl");
    symlink(&victim, &events).unwrap();
    assert!(run_hook(&script, &fixture.worktree, &envs, &payload)
        .status
        .success());
    assert_eq!(fs::read_to_string(victim).unwrap(), "unchanged");
    assert!(!HOOK_WORKTREE_FILE_GUARD.contains("worktree-file-guard-debug"));
}
