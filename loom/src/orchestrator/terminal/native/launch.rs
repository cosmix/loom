//! Shared session-launch preparation for the native and tmux backends.
//!
//! Builds everything a spawn needs UP TO the point of actually starting a
//! process: the resolved tracking key / PID-file key, the kind-specific
//! prompt, the model/effort/permission-mode policy, the session's settings
//! capsule and scratch directory, and the wrapper script that `exec`s claude.
//! Both [`super::NativeBackend::spawn`] and
//! [`super::super::tmux::TmuxBackend::spawn`] call this so the two lanes can
//! never silently diverge on any of these behaviors.

mod host;

use anyhow::Result;
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::sandbox::MergedSandboxConfig;

use super::session_settings::{write_session_capsule, CapsuleRequest};
use super::SessionCapsule;
use host::LaunchHost;

/// Derive the Remote Control session name for a spawn, prefixed by kind.
///
/// Base is `stage.name` (required by the plan schema); falls back to
/// `stage.id` when `stage.name` is empty after trimming (hand-built stages
/// with no name set).
fn remote_control_session_name(kind: SessionType, stage: &Stage) -> String {
    let trimmed = stage.name.trim();
    let base = if trimmed.is_empty() {
        stage.id.as_str()
    } else {
        trimmed
    };
    match kind {
        SessionType::Stage => base.to_string(),
        SessionType::Merge => format!("Merge: {base}"),
        SessionType::BaseConflict => format!("Base conflict: {base}"),
        SessionType::Knowledge => format!("Knowledge: {base}"),
        SessionType::Adjudication => format!("Adjudication: {base}"),
    }
}

/// Whether `.loom/work/config.toml`'s `[context]` table has `prompt_cache_split =
/// true`. Defaults to `false`: the split ships DISABLED by default (no env
/// var, no heuristic) — only this explicit config key turns it on.
fn prompt_cache_split_enabled(work_dir: &Path) -> bool {
    let Ok(Some(mut config)) = crate::fs::work_dir::load_config(work_dir) else {
        return false;
    };
    config
        .as_toml_mut()
        .get("context")
        .and_then(|table| table.get("prompt_cache_split"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// Resolve the path to hand `claude` via `--append-system-prompt-file`, or
/// `None` when the split is off (the default) or the prefix file could not be
/// written.
///
/// Enabled, this writes the stage's stable prefix
/// ([`crate::orchestrator::signals::stable_prefix_for`]) to
/// `<work_dir>/signals/prefix/<stage-id>.md` and returns that path, so the
/// immutable half of the signal is handed over as its own cacheable unit
/// instead of being re-sent behind the volatile half on every turn. The file
/// is the same string the signal's stable section renders, byte for byte,
/// never an approximation of it — including the pointer to `~/.claude/CLAUDE.md`
/// that now stands in for the doctrine this stage moved out of the signal.
///
/// Every failure degrades to `None`, which produces exactly the command line
/// the split-off default produces: this is an optimisation, and a failed
/// optimisation must never fail the spawn.
fn resolve_prompt_cache_split_prefix_file(work_dir: &Path, stage: &Stage) -> Option<String> {
    if !prompt_cache_split_enabled(work_dir) {
        return None;
    }
    let prefix_dir = work_dir.join("signals").join("prefix");
    if let Err(error) = std::fs::create_dir_all(&prefix_dir) {
        tracing::debug!(stage_id = %stage.id, %error, "could not create the prefix directory; launching without the prompt-cache split");
        return None;
    }
    let prefix_file = prefix_dir.join(format!("{}.md", stage.id));
    let prefix = crate::orchestrator::signals::stable_prefix_for(stage.stage_type);
    if let Err(error) = crate::fs::locking::locked_write(&prefix_file, &prefix) {
        tracing::debug!(stage_id = %stage.id, %error, "could not write the stable prefix; launching without the prompt-cache split");
        return None;
    }
    Some(prefix_file.to_str()?.to_string())
}

/// Model and reasoning effort POLICY per session kind (kept explicit, not
/// buried in the spawn).
///
/// * Merge and base-conflict resolution always run on the strongest model at
///   high reasoning effort, regardless of the originating stage's settings.
/// * Adjudication judges a criterion the stage's own agent could not satisfy,
///   and running it on the disputing stage's model would let a plan pick its
///   own judge — so it uses the adjudicator's own model
///   (`.loom/work/config.toml::[adjudication] model`, default `opus`).
/// * Stage and knowledge sessions resolve through the four-tier chain in
///   `crate::fs::work_dir::resolve_stage_model_effort`: the stage's own plan
///   fields, then `.loom/work/config.toml`'s `[models]`, then
///   `~/.loom/config.toml`'s `[models]`, then the stage type's built-in.
fn model_and_effort(kind: SessionType, stage: &Stage, work_dir: &Path) -> (String, String) {
    match kind {
        SessionType::Merge | SessionType::BaseConflict => ("opus".to_string(), "high".to_string()),
        SessionType::Adjudication => (
            crate::orchestrator::adjudication::resolve_model(work_dir),
            "high".to_string(),
        ),
        SessionType::Stage | SessionType::Knowledge => {
            crate::fs::work_dir::resolve_stage_model_effort(
                work_dir,
                stage.stage_type,
                stage.model.as_deref(),
                stage.reasoning_effort.as_deref(),
            )
        }
    }
}

/// The kind-specific initial prompt, pointing the session at its signal file.
fn initial_prompt(kind: SessionType, stage: &Stage, signal_path: &Path) -> String {
    let signal_path_str = signal_path.to_string_lossy();
    match kind {
        SessionType::Stage => {
            // The literal keyword "ultracode" in the prompt is what licenses
            // Claude Code's Workflow tool for the session.
            let ultracode_suffix = if stage.ultracode {
                " This stage is licensed for ultracode workflow orchestration."
            } else {
                ""
            };
            format!(
                "Read the signal file at {signal_path_str} and execute the assigned stage work. \
                 This file contains your assignment, tasks, acceptance criteria, \
                 and context files to read.{ultracode_suffix}"
            )
        }
        SessionType::Merge => format!(
            "Read the merge signal file at {signal_path_str} and resolve the merge conflicts. \
             This file contains the conflicting files, merge context, and resolution instructions."
        ),
        SessionType::BaseConflict => format!(
            "Read the base conflict signal file at {signal_path_str} and resolve the merge conflicts. \
             This file contains the conflicting files from merging dependency branches, \
             and instructions for resolution. After resolving, tell the user to run `loom retry {}`.",
            stage.id
        ),
        SessionType::Knowledge => format!(
            "Read the signal file at {signal_path_str} and execute the assigned knowledge gathering work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and instructions for populating the knowledge base."
        ),
        SessionType::Adjudication => format!(
            "Read the adjudication signal file at {signal_path_str} and judge the disputed \
             acceptance criterion. This file contains the dispute, the evidence available to \
             you, and the command that records your verdict. Judge the dispute; change nothing."
        ),
    }
}

/// The stage's sandbox config, merged and path-expanded as the spawn paths
/// merge it before writing settings.
///
/// Resolved from the `[plan_sandbox]` snapshot the settings generator reads
/// (OrchestratorConfig.sandbox_config is loaded from it too), so the
/// `--permission-mode` flag and the capsule never disagree. Loom stages run
/// autonomously with no human at the terminal, so they must START in the
/// resolved mode (default: `auto`): Claude Code v2.1.142+ ignores
/// `defaultMode: "auto"` from settings files, and only `--permission-mode` is
/// honored (see `build_claude_command`).
fn session_sandbox(work_dir: &Path, stage: &Stage) -> MergedSandboxConfig {
    let plan_sandbox = crate::fs::work_dir::read_plan_sandbox(work_dir)
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut sandbox = crate::sandbox::merge_config(
        &plan_sandbox,
        &stage.sandbox,
        stage.stage_type,
        &stage.implementers,
    );
    crate::sandbox::expand_paths(&mut sandbox);
    sandbox
}

/// The `claude` invocation for this launch; `build_claude_command`
/// shell-escapes the path, model, effort, mode and remote-control name (S-3).
/// The claude path is absolute, since macOS terminals do not inherit PATH.
fn claude_command(
    host: &LaunchHost,
    work_dir: &Path,
    kind: SessionType,
    stage: &Stage,
    signal_path: &Path,
    sandbox: &MergedSandboxConfig,
    capsule: &SessionCapsule,
) -> String {
    let prompt = initial_prompt(kind, stage, signal_path);
    let escaped_prompt = escape(Cow::Borrowed(&prompt));
    let (model, effort) = model_and_effort(kind, stage, work_dir);
    let rc_name = remote_control_session_name(kind, stage);
    let remote_control = crate::remote_control::resolve_invocation(work_dir, &rc_name);
    super::build_claude_command(
        &host.claude_path.display().to_string(),
        &model,
        &effort,
        sandbox.permission_mode.as_settings_value(),
        capsule,
        &remote_control,
        &escaped_prompt,
    )
}

/// Prepare everything needed to launch a session, short of actually starting
/// the terminal/tmux process.
///
/// Returns `(session, title, pid_key, wrapper_path_abs)`:
/// * `session` — the input session with `session_type` and the stage
///   assignment (and therefore `tracking_key`) applied.
/// * `title` — the window title / tmux session name (`session.tracking_key`).
/// * `pid_key` — the per-session PID-file key (`title + "-" + session.id`).
/// * `wrapper_path_abs` — the absolute path to the wrapper script that
///   `exec`s claude.
///
/// The host facts (claude, the verified hooks directory and loom binary, the
/// scratch root, the filtered PATH) are resolved here from the daemon's own
/// environment; everything after that is [`prepare_session_launch_with`].
pub(crate) fn prepare_session_launch(
    work_dir: &Path,
    kind: SessionType,
    stage: &Stage,
    session: Session,
    signal_path: &Path,
    cwd: &Path,
) -> Result<(Session, String, String, PathBuf)> {
    let host = LaunchHost::from_env(work_dir, cwd)?;
    prepare_session_launch_with(&host, work_dir, kind, stage, session, signal_path, cwd)
}

/// [`prepare_session_launch`] against already-resolved host facts, which
/// tests supply directly.
///
/// The stage is assigned first so `tracking_key` is set for the (stage, kind)
/// pair: the window title IS the tracking_key, and the per-session PID-file
/// key (tracking_key + session.id, so two consecutive sessions for one stage
/// never share a PID file, O-14) derives from it. The kind prefix stops at OS
/// resources and never reaches `LOOM_STAGE_ID`, which is why the wrapper gets
/// `stage.id`. The context ceiling resolves stage value, then
/// `[context] ceiling_tokens`, then the default, so the window the session
/// runs under is the number its signal quotes.
fn prepare_session_launch_with(
    host: &LaunchHost,
    work_dir: &Path,
    kind: SessionType,
    stage: &Stage,
    mut session: Session,
    signal_path: &Path,
    cwd: &Path,
) -> Result<(Session, String, String, PathBuf)> {
    session.session_type = kind;
    session.assign_to_stage(stage.id.clone());
    let title = session.tracking_key.clone();
    let pid_key = format!("{}-{}", title, session.id);
    let sandbox = session_sandbox(work_dir, stage);
    let scratch_dir = host.prepare_scratch(&session.id)?;
    let settings_file = write_session_capsule(&CapsuleRequest {
        kind,
        session_id: &session.id,
        sandbox: &sandbox,
        cwd,
        work_dir,
        repo_root: &host.repo_root,
        hooks_dir: host.hooks_dir.as_deref(),
        scratch_dir: &scratch_dir,
        surfaces: &host.control_surfaces(work_dir),
    })?;
    let prefix_file = resolve_prompt_cache_split_prefix_file(work_dir, stage);
    let capsule = super::session_capsule(host.capsule_support, Some(settings_file), prefix_file);
    let claude_cmd = claude_command(host, work_dir, kind, stage, signal_path, &sandbox, &capsule);
    let context_ceiling_tokens =
        crate::fs::work_dir::resolve_context_ceiling_tokens(work_dir, stage.context_ceiling_tokens);
    let wrapper_path = super::wrapper::create_session_wrapper_script(
        work_dir,
        &pid_key,
        &stage.id,
        &session.id,
        &claude_cmd,
        Some(cwd),
        kind,
        context_ceiling_tokens,
        super::build_cache::sccache_usable_in(&sandbox),
        &host.wrapper_env(scratch_dir),
    )?;
    // Absolute: macOS terminals open in the home directory.
    let wrapper_path_abs = wrapper_path.canonicalize().unwrap_or(wrapper_path);
    Ok((session, title, pid_key, wrapper_path_abs))
}

#[cfg(test)]
#[path = "tests_launch.rs"]
mod tests;
#[cfg(test)]
#[path = "tests_launch_capsule.rs"]
mod tests_launch_capsule;
