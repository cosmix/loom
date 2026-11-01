//! Integration tests for the read-guard PreToolUse hook.

use super::helpers::clear_relay_env;
use loom::fs::permissions::constants::{
    HOOK_COMMON, HOOK_READ_DISCIPLINE, HOOK_READ_GUARD, HOOK_READ_LEDGER,
};
use loom::process::sandbox_probe::{process_tree_visible, skip_unless};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::TempDir;

const STUB_STDERR_WARNING: &str = "could not refresh the working-tree source graph";

struct HookOutput {
    code: i32,
    stdout: String,
    stderr: String,
}

fn write_exec(path: &Path, content: &str) {
    fs::write(path, content).expect("write file");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn setup_hook() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("create temp dir");
    write_exec(
        &temp.path().join("_bounded.sh"),
        loom::fs::permissions::constants::HOOK_BOUNDED,
    );
    write_exec(&temp.path().join("_common.sh"), HOOK_COMMON);
    write_exec(
        &temp.path().join("_parser.sh"),
        loom::fs::permissions::constants::HOOK_PARSER,
    );
    write_exec(
        &temp.path().join("_parser_expand.sh"),
        loom::fs::permissions::constants::HOOK_PARSER_EXPAND,
    );
    write_exec(
        &temp.path().join("_parser_scan.sh"),
        loom::fs::permissions::constants::HOOK_PARSER_SCAN,
    );
    write_exec(
        &temp.path().join("_identity.sh"),
        loom::fs::permissions::constants::HOOK_IDENTITY,
    );
    write_exec(
        &temp.path().join("_stage_lookup.sh"),
        loom::fs::permissions::constants::HOOK_STAGE_LOOKUP,
    );
    write_exec(
        &temp.path().join("_containment.sh"),
        loom::fs::permissions::constants::HOOK_CONTAINMENT,
    );
    write_exec(
        &temp.path().join("_heartbeat.sh"),
        loom::fs::permissions::constants::HOOK_HEARTBEAT,
    );
    write_exec(
        &temp.path().join("_spawn_policy.sh"),
        loom::fs::permissions::constants::HOOK_SPAWN_POLICY,
    );
    write_exec(
        &temp.path().join("_read_discipline.sh"),
        HOOK_READ_DISCIPLINE,
    );
    write_exec(&temp.path().join("_read_ledger.sh"), HOOK_READ_LEDGER);
    let hook_path = temp.path().join("read-guard.sh");
    write_exec(&hook_path, HOOK_READ_GUARD);
    (temp, hook_path)
}

fn write_file_with_lines(dir: &Path, name: &str, lines: usize) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, "line\n".repeat(lines)).expect("write test file");
    path
}

fn write_loom_stub(dir: &Path, body: &str) {
    let script = format!(
        "#!/usr/bin/env bash\ncat <<'EOF'\n{body}\nEOF\necho 'warning: {STUB_STDERR_WARNING} (stub)' >&2\n"
    );
    write_exec(&dir.join("loom"), &script);
}

fn covered_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-covered");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(
        &dir,
        "-> Outline: stub\n\tL14-L14\tconstant\tPOLL_INTERVAL\tconst POLL_INTERVAL: ...\n\tL40-L61\tfunction\tgather\tfn gather(\ncoverage: full",
    );
    dir
}

fn uncovered_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-uncovered");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(
        &dir,
        "-> Outline: stub\ncoverage: lexical-only - no source-graph extractor for sh",
    );
    dir
}

fn lexical_stub_dir(root: &Path) -> PathBuf {
    let dir = root.join("stub-lexical");
    fs::create_dir_all(&dir).expect("create stub dir");
    write_loom_stub(&dir, "coverage: lexical-only");
    dir
}

struct Session {
    work: TempDir,
    session_id: &'static str,
    stage_id: Option<&'static str>,
    agent_id: &'static str,
    main_agent_pid: Option<String>,
}

impl Session {
    fn new() -> Self {
        Session {
            work: TempDir::new().expect("create work dir"),
            session_id: "sess-1",
            stage_id: Some("stage-1"),
            agent_id: "agent-1",
            main_agent_pid: None,
        }
    }

    fn work_dir(&self) -> &Path {
        self.work.path()
    }

    fn enable_deny(&self) {
        fs::write(
            self.work_dir().join("config.toml"),
            "[hooks]\ndeny_enabled = true\n",
        )
        .expect("write config.toml");
    }

    fn with_live_main_agent(mut self) -> Self {
        self.main_agent_pid = Some(std::process::id().to_string());
        self
    }

    fn with_stage_id(mut self, stage_id: Option<&'static str>) -> Self {
        self.stage_id = stage_id;
        self
    }
}

fn configure_command_environment(
    command: &mut Command,
    session: &Session,
    stub_dir: Option<&Path>,
) {
    let path_value = match stub_dir {
        Some(dir) => format!(
            "{}:{}",
            dir.display(),
            std::env::var("PATH").unwrap_or_default()
        ),
        None => std::env::var("PATH").unwrap_or_default(),
    };
    clear_relay_env(command)
        .env("PATH", path_value)
        .env("LOOM_WORK_DIR", session.work_dir())
        .env("LOOM_SESSION_ID", session.session_id)
        .env("LOOM_HOME", session.work_dir().join("loom-home"))
        .env("TMPDIR", session.work_dir());
    if let Some(stage_id) = session.stage_id {
        command.env("LOOM_STAGE_ID", stage_id);
    } else {
        command.env_remove("LOOM_STAGE_ID");
    }
}

fn run_payload(
    hook: &Path,
    payload: &Value,
    session: &Session,
    stub_dir: Option<&Path>,
) -> HookOutput {
    let payload_str = payload.to_string();
    let mut cmd = Command::new("bash");
    cmd.arg(hook);
    configure_command_environment(&mut cmd, session, stub_dir);
    cmd.env_remove("LOOM_HOOK_DEBUG")
        .env_remove("COMMIT_FILTER_DEBUG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(pid) = &session.main_agent_pid {
        cmd.env("LOOM_MAIN_AGENT_PID", pid);
    }

    let mut child = cmd.spawn().expect("spawn hook");
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(payload_str.as_bytes()).ok();
    }
    let output = child.wait_with_output().expect("wait for hook");
    HookOutput {
        code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

fn run_read_hook(
    hook: &Path,
    tool_input: Value,
    session: &Session,
    stub_dir: Option<&Path>,
) -> HookOutput {
    let payload = json!({
        "tool_name": "Read",
        "tool_input": tool_input,
        "agent_id": session.agent_id,
        "session_id": session.session_id,
        "cwd": session.work_dir(),
    });
    run_payload(hook, &payload, session, stub_dir)
}

fn skip_unless_gate_visible(test: &str) -> bool {
    skip_unless(
        process_tree_visible(),
        &format!("hooks_read_guard::{test}"),
        "the guard's deny branch needs a visible process tree",
    )
}

fn warn_context(stdout: &str) -> String {
    let v: Value = serde_json::from_str(stdout.trim()).expect("parse stdout json");
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext present")
        .to_string()
}

#[path = "hooks_read_guard_repeat.rs"]
mod repeat;

#[path = "hooks_read_guard_cases.rs"]
mod cases;
