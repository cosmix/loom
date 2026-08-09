//! Adversarial regression tests for security-sensitive hook policy.

use crate::fs::permissions::constants::{
    HOOK_CODEX_FORWARD_GUARD, HOOK_COMMON, HOOK_POST_TOOL_USE, HOOK_WORKTREE_FILE_GUARD,
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
        let work_dir = repo.join(".work");

        for path in [&hooks, &worktree, &sibling, &home, &work_dir] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(hooks.join("_common.sh"), HOOK_COMMON).unwrap();
        fs::write(&outside, "outside").unwrap();
        fs::write(sibling.join("file.txt"), "sibling").unwrap();
        symlink("../../.work", worktree.join(".work")).unwrap();

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
        file_call(&fixture, "Read", ".work/admin.token")
            .status
            .code(),
        Some(2)
    );
    assert!(file_call(&fixture, "Write", ".work/handoffs/state.md")
        .status
        .success());
    for protected in [
        ".work/memory/forged.md",
        ".work/disputes/stage/1/verdict.md",
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
        &[("HOME", &fixture.home)],
        &payload,
    )
}

#[test]
fn forward_guard_allows_only_exact_forward_wrapper_command() {
    let fixture = HookFixture::new();
    let command = "~/.claude/hooks/loom/codex-forward.sh task 'hello; literal' --model gpt-5.6-terra --effort xhigh --write";
    let payload = json!({
        "tool_name": "Bash",
        "agent_type": "loom-codex-forwarder",
        "tool_input": {"command": command}
    });
    assert!(forward_call(&fixture, payload).status.success());
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
    assert_eq!(
        fs::metadata(&heartbeat).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let legacy_events = fixture.work_dir.join("tool-events.jsonl");
    fs::write(&legacy_events, "legacy telemetry remains untouched").unwrap();
    assert!(run_hook(&script, &fixture.worktree, &envs, &payload)
        .status
        .success());
    assert_eq!(
        fs::read_to_string(legacy_events).unwrap(),
        "legacy telemetry remains untouched"
    );
    assert!(!HOOK_WORKTREE_FILE_GUARD.contains("worktree-file-guard-debug"));
}
