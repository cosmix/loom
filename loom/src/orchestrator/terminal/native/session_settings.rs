//! Per-judge Claude Code settings, so an adjudication session gets the
//! PostToolUse heartbeat hook even though it runs in the main repository
//! rather than a loom worktree.
//!
//! A judge (`SessionType::Adjudication`) never goes through
//! `hooks::generator::setup_hooks_for_worktree` — that call is worktree-only
//! (`core/stage_executor.rs`'s `install_required_hooks`) — so it inherits
//! whatever `.claude/settings.local.json` the operator's own main repo
//! already has, whose hook set (`fs/permissions/hooks.rs::loom_hooks_config`)
//! has no `post-tool-use.sh`. Without that hook the daemon's stall watchdog
//! (`monitor/detection.rs`, `core/event_handler/stalled_judge.rs`) has no
//! heartbeat to read and falls back to the session's `created_at`, so a judge
//! that legitimately runs longer than the stage's idle budget gets killed.
//!
//! This module builds a settings document that layers ONLY that one hook on
//! top of whatever the judge's `cwd` already has, writes it to a
//! session-scoped capsule file under `<work_dir>/capsules/` so the judge
//! never touches the operator's own settings file, and resolves the
//! `--settings` path a launch should pin per session kind.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use crate::hooks::HookEvent;
use crate::models::session::{Session, SessionType};
use crate::validation::validate_id;

/// `<work_dir>/capsules/` — where per-session generated settings files live.
fn capsules_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("capsules")
}

/// The path a session's generated settings file is (or would be) written to.
///
/// `pub(crate)`, not `pub(super)`: `core/judge_close.rs` and
/// `core/event_handler/stalled_judge_tests.rs` need it too (via the
/// re-export in `native/mod.rs`), the same way `judge_heartbeat_path` is
/// public for cross-module use.
pub(crate) fn session_settings_path(work_dir: &Path, session_id: &str) -> PathBuf {
    capsules_dir(work_dir).join(format!("{session_id}.settings.json"))
}

/// Whether a `PostToolUse` array already names `post-tool-use.sh`.
fn has_post_tool_use_hook(entries: &[Value]) -> bool {
    entries.iter().any(|entry| {
        entry
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|hooks| {
                hooks.iter().any(|hook| {
                    hook.get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|command| command.ends_with("post-tool-use.sh"))
                })
            })
    })
}

/// Layer the PostToolUse heartbeat hook onto `base` (or a fresh settings
/// document when there is none), scrub per-session identity env the way
/// every worktree settings file already does, and return the result.
///
/// Adds ONLY the heartbeat hook, never the full worktree hook set: a judge's
/// working directory is the main repo, not a worktree, so hooks that assume
/// one (session-start's worktree bootstrap, session-end's cleanup, ...) do
/// not belong here — a judge only needs to be seen alive.
pub(super) fn with_post_tool_use_hook(base: Option<&Value>, hooks_dir: &Path) -> Value {
    let mut settings = match base {
        Some(value) if value.is_object() => value.clone(),
        _ => json!({}),
    };

    let hooks = settings
        .as_object_mut()
        .expect("settings was just ensured to be an object")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let post_tool_use = hooks
        .as_object_mut()
        .expect("hooks was just ensured to be an object")
        .entry(HookEvent::PostToolUse.to_string())
        .or_insert_with(|| json!([]));
    if !post_tool_use.is_array() {
        *post_tool_use = json!([]);
    }
    let entries = post_tool_use
        .as_array_mut()
        .expect("post_tool_use was just ensured to be an array");

    if !has_post_tool_use_hook(entries) {
        let command = hooks_dir
            .join(HookEvent::PostToolUse.script_name())
            .display()
            .to_string();
        entries.push(json!({
            "matcher": "*",
            "hooks": [{"type": "command", "command": command}]
        }));
    }

    crate::fs::permissions::scrub_session_identity_env(&mut settings);
    crate::fs::permissions::scrub_stale_work_dir_env(&mut settings);

    settings
}

/// Read `<cwd>/.claude/settings.local.json` as the base document for
/// [`with_post_tool_use_hook`], or `None` if there is none to layer onto.
///
/// A file that exists but fails to parse is an error, not a silent `{}`: a
/// judge must never start under a settings capsule that silently dropped
/// whatever the operator's own file carried.
fn read_base_settings(cwd: &Path) -> Result<Option<Value>> {
    let path = cwd.join(".claude").join("settings.local.json");
    if !path.is_file() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let value: Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {} as JSON", path.display()))?;
    Ok(Some(value))
}

/// Write a session's generated settings capsule and return its path.
///
/// The file layers the PostToolUse heartbeat hook onto whatever `cwd`'s own
/// `.claude/settings.local.json` already has (so a judge keeps every other
/// hook and permission the operator's repo carries), and is written under
/// `<work_dir>/capsules/`, never into `cwd`, so it can never collide with or
/// leak into the operator's own file. Written atomically via
/// [`crate::fs::locking::locked_write`] (temp file + `fsync` + `rename`), the
/// same primitive every other `.loom/work/` settings-shaped write uses.
pub(super) fn write_session_settings(
    work_dir: &Path,
    session_id: &str,
    cwd: &Path,
    hooks_dir: &Path,
) -> Result<PathBuf> {
    validate_id(session_id)
        .with_context(|| format!("invalid session id for a settings capsule: {session_id}"))?;

    let base = read_base_settings(cwd)?;
    let settings = with_post_tool_use_hook(base.as_ref(), hooks_dir);
    let content = serde_json::to_string_pretty(&settings)
        .context("failed to serialize the session's settings capsule")?;

    let dir = capsules_dir(work_dir);
    if !dir.exists() {
        // Mirrors `fs::work_dir::ensure_layout`'s 0o700 state-directory
        // subdirectories: this file carries session-scoped hook config, so it
        // gets the same private mode as the rest of `.loom/work/`, not the
        // process umask a plain `create_dir_all` would leave it at.
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }

    let path = session_settings_path(work_dir, session_id);
    crate::fs::locking::locked_write(&path, &content)
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(path)
}

/// Remove a session's generated settings capsule, if any.
///
/// Best-effort, mirroring [`crate::orchestrator::monitor::heartbeat::cleanup_judge_heartbeat`]:
/// a missing file is not an error, since cleanup can race a judge that never
/// got far enough to have one written.
pub(crate) fn cleanup_session_settings(work_dir: &Path, session_id: &str) {
    let path = session_settings_path(work_dir, session_id);
    if let Err(error) = fs::remove_file(&path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                session_id = %session_id,
                path = %path.display(),
                %error,
                "failed to remove the session's generated settings capsule",
            );
        }
    }
}

/// The `--settings` path a launch should pin for `kind`: a freshly generated
/// capsule carrying the PostToolUse heartbeat hook for an adjudication
/// session (see the module doc comment for why judges need one of their
/// own), or `cwd`'s own `.claude/settings.local.json` for every other kind —
/// exactly what `session_capsule` resolved internally before this module
/// existed.
///
/// A judge whose hooks directory cannot be resolved still launches — with a
/// warning — under `cwd`'s settings rather than failing the spawn outright;
/// the heartbeat hook is a liveness aid, not a security boundary a missing
/// installation should block a judge over.
pub(super) fn resolve_settings_file(
    kind: SessionType,
    cwd: &Path,
    work_dir: &Path,
    session_id: &str,
) -> Result<Option<String>> {
    if kind != SessionType::Adjudication {
        return Ok(super::capsule::resolved_settings_file(cwd));
    }
    let Some(hooks_dir) = crate::hooks::find_hooks_dir() else {
        tracing::warn!(
            session_id = %session_id,
            "no loom hooks directory installed; the judge will run without the heartbeat hook",
        );
        return Ok(super::capsule::resolved_settings_file(cwd));
    };
    let path = write_session_settings(work_dir, session_id, cwd, &hooks_dir)?;
    let path_str = path.to_str().ok_or_else(|| {
        anyhow::anyhow!(
            "session settings capsule path is not valid UTF-8: {}",
            path.display()
        )
    })?;
    Ok(Some(path_str.to_string()))
}

/// Resolve the [`super::SessionCapsule`] for a launch.
///
/// Reads `session.session_type` and `session.id` directly rather than taking
/// `kind`/`session_id` as separate parameters: by the time
/// `prepare_session_launch` reaches this call it has already stamped
/// `session.session_type = kind`, so the two never diverge, and folding them
/// into one parameter keeps the call a single line under the file's pinned
/// `prepare_session_launch` length.
pub(super) fn capsule_for(
    claude_path: &Path,
    cwd: &Path,
    work_dir: &Path,
    session: &Session,
    append_system_prompt_file: Option<String>,
) -> Result<super::SessionCapsule> {
    let settings_file = resolve_settings_file(session.session_type, cwd, work_dir, &session.id)?;
    Ok(super::session_capsule(
        claude_path,
        settings_file,
        append_system_prompt_file,
    ))
}
