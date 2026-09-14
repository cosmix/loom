use anyhow::{ensure, Context, Result};
use chrono::{SecondsFormat, Utc};
use loom::codex_lifecycle::CodexAuthorization;
use loom::fs::permissions::constants::{
    HOOK_CODEX_FORWARD, HOOK_CODEX_FORWARD_COMMON, HOOK_CODEX_FORWARD_GUARD, HOOK_COMMON,
    HOOK_LIFECYCLE,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::fake_companion;
use crate::fixture::{Forwarder, EFFORT, LOOM_SESSION, MODEL, STAGE};

pub fn install_hooks(hooks: &Path) -> Result<()> {
    for (name, contents) in [
        ("_common.sh", HOOK_COMMON),
        ("_codex_forward.sh", HOOK_CODEX_FORWARD_COMMON),
        ("_lifecycle.sh", HOOK_LIFECYCLE),
        ("codex-forward-guard.sh", HOOK_CODEX_FORWARD_GUARD),
        ("codex-forward.sh", HOOK_CODEX_FORWARD),
    ] {
        write_executable(&hooks.join(name), contents)?;
    }
    Ok(())
}

pub fn install_fake_companion(home: &Path) -> Result<()> {
    let path =
        home.join(".claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs");
    fs::create_dir_all(path.parent().context("fake companion parent")?)?;
    fs::write(path, fake_companion::SCRIPT)?;
    Ok(())
}

fn write_executable(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

pub fn write_stage(work: &Path, project: &Path) -> Result<()> {
    fs::create_dir_all(work.join("stages"))?;
    let content = format!(
        "---\nid: {STAGE}\nname: Codex evidence\ndescription: Fixture\nstatus: executing\ndependencies: []\nparallel_group: null\nacceptance: []\nfiles: []\nplan_id: null\nworktree: {}\nsession: {LOOM_SESSION}\nparent_stage: null\nchild_stages: []\ncreated_at: \"2026-09-14T10:00:00Z\"\nupdated_at: \"2026-09-14T10:00:00Z\"\ncompleted_at: null\nclose_reason: null\n---\n",
        project.display()
    );
    fs::write(work.join("stages").join(format!("01-{STAGE}.md")), content)?;
    Ok(())
}

pub fn write_session(work: &Path, project: &Path) -> Result<()> {
    fs::create_dir_all(work.join("sessions"))?;
    let content = format!(
        "---\nid: {LOOM_SESSION}\nstage_id: {STAGE}\nworktree_path: {}\npid: null\nstatus: running\ncontext_tokens: 0\ncreated_at: \"2026-09-14T10:00:00Z\"\nlast_active: \"2026-09-14T10:00:00Z\"\n---\n",
        project.display()
    );
    fs::write(
        work.join("sessions").join(format!("{LOOM_SESSION}.md")),
        content,
    )?;
    Ok(())
}

pub fn updated_command(output: &Output) -> Result<String> {
    let value: Value = serde_json::from_slice(&output.stdout)?;
    value["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str()
        .map(str::to_owned)
        .context("guard omitted updatedInput.command")
}

pub fn guard_payload(
    forwarder: &Forwarder,
    command: &str,
    agent_override: Option<&str>,
    project: &Path,
) -> Value {
    json!({
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_use_id": format!("tool-{}", forwarder.agent),
        "session_id": forwarder.parent,
        "agent_id": agent_override.unwrap_or(&forwarder.agent),
        "agent_type": "loom-codex-forwarder",
        "transcript_path": forwarder.transcript,
        "cwd": project,
        "tool_input": {"command": command}
    })
}

pub fn assert_authorization(
    authorization: &CodexAuthorization,
    forwarder: &Forwarder,
    unit: &str,
    home: &Path,
    project: &Path,
) -> Result<()> {
    assert_authorization_identity(authorization, forwarder, unit)?;
    assert_authorization_adapter(authorization, home, project)
}

fn assert_authorization_identity(
    authorization: &CodexAuthorization,
    forwarder: &Forwarder,
    unit: &str,
) -> Result<()> {
    ensure!(
        authorization.stage_id == STAGE,
        "authorization stage mismatch"
    );
    ensure!(
        authorization.loom_session_id == LOOM_SESSION,
        "Loom session mismatch"
    );
    ensure!(
        authorization.parent_session_id == forwarder.parent,
        "parent mismatch"
    );
    ensure!(
        authorization.forwarder_agent_id == forwarder.agent,
        "forwarder mismatch"
    );
    ensure!(
        authorization.tool_use_id == format!("tool-{}", forwarder.agent),
        "tool mismatch"
    );
    ensure!(authorization.unit_id == unit, "authorization unit mismatch");
    Ok(())
}

fn assert_authorization_adapter(
    authorization: &CodexAuthorization,
    home: &Path,
    project: &Path,
) -> Result<()> {
    ensure!(
        authorization.model == MODEL && authorization.effort == EFFORT,
        "request mismatch"
    );
    ensure!(
        authorization.workspace_root == project,
        "workspace mismatch"
    );
    ensure!(
        authorization.companion_version == "1.0.6",
        "companion version mismatch"
    );
    ensure!(
        authorization.effective_state_root == home.join(".codex/plugin-data/state"),
        "state root mismatch"
    );
    ensure!(
        authorization.selected_companion
            == home
                .join(".claude/plugins/cache/openai-codex/codex/1.0.6/scripts/codex-companion.mjs"),
        "companion path mismatch"
    );
    Ok(())
}

pub fn workspace_state_dir(state_root: &Path, workspace: &Path) -> PathBuf {
    let basename = workspace
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("workspace");
    let mut slug = String::new();
    for character in basename.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            slug.push(character);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "workspace" } else { slug };
    let hash = hex::encode(Sha256::digest(workspace.to_string_lossy().as_bytes()));
    state_root.join(format!("{slug}-{}", &hash[..16]))
}

pub fn apply_status(job: &mut Value, status: &str, thread_id: &str, turn_id: &str) {
    let terminal = matches!(status, "completed" | "failed" | "cancelled");
    // Stamp a real "now" rather than the fixture's fixed creation timestamp:
    // the codex stall detector reads `updatedAt` against wall-clock time, and
    // a job just transitioned to a status is, by definition, freshly updated.
    job["updatedAt"] = json!(Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true));
    job["status"] = json!(status);
    job["phase"] = json!(match status {
        "completed" => "done",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "queued" => "queued",
        _ => "starting",
    });
    job["threadId"] = terminal.then_some(json!(thread_id)).unwrap_or(Value::Null);
    job["turnId"] = terminal.then_some(json!(turn_id)).unwrap_or(Value::Null);
    job["completedAt"] = terminal
        .then_some(json!("2026-09-14T10:01:00.000Z"))
        .unwrap_or(Value::Null);
    job["errorMessage"] = match status {
        "failed" => json!("companion failed"),
        "cancelled" => json!("companion cancelled"),
        _ => Value::Null,
    };
    job["result"] = terminal
        .then_some(json!({"text": "terminal fixture"}))
        .unwrap_or(Value::Null);
}

pub fn output_text(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub struct ProcessEnvironment {
    home: Option<OsString>,
    tmpdir: Option<OsString>,
}

impl ProcessEnvironment {
    pub fn set(home: &Path, tmpdir: &Path) -> Self {
        let saved = Self {
            home: std::env::var_os("HOME"),
            tmpdir: std::env::var_os("TMPDIR"),
        };
        std::env::set_var("HOME", home);
        std::env::set_var("TMPDIR", tmpdir);
        saved
    }
}

impl Drop for ProcessEnvironment {
    fn drop(&mut self) {
        restore_var("HOME", self.home.take());
        restore_var("TMPDIR", self.tmpdir.take());
    }
}

fn restore_var(name: &str, value: Option<OsString>) {
    if let Some(value) = value {
        std::env::set_var(name, value);
    } else {
        std::env::remove_var(name);
    }
}
