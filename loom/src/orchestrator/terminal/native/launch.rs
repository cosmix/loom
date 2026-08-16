//! Shared session-launch preparation for the native and tmux backends.
//!
//! Builds everything a spawn needs UP TO the point of actually starting a
//! process: the resolved tracking key / PID-file key, the kind-specific
//! prompt, the model/effort/permission-mode policy, and the wrapper script
//! that `exec`s claude. Both [`super::NativeBackend::spawn`] and
//! [`super::super::tmux::TmuxBackend::spawn`] call this so the two lanes can
//! never silently diverge on any of these six behaviors.

use anyhow::Result;
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::claude::find_claude_path;
use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;

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
    }
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
pub(crate) fn prepare_session_launch(
    work_dir: &Path,
    kind: SessionType,
    stage: &Stage,
    session: Session,
    signal_path: &Path,
    cwd: &Path,
) -> Result<(Session, String, String, PathBuf)> {
    // Assign the stage first so `tracking_key` is set for the (stage, kind)
    // pair; the window title IS the tracking_key and the PID-file key is
    // derived from it. (Idempotent for knowledge sessions, which derived it
    // at construction.)
    let mut session = session;
    session.session_type = kind;
    session.assign_to_stage(stage.id.clone());

    // Window title. `tracking_key` is `loom-[<kind>-]<stage-id>`; the kind
    // prefix namespaces OS resources and stops there — it must never reach
    // `LOOM_STAGE_ID`, which is why `stage.id` is passed below (see wrapper.rs).
    let title = session.tracking_key.clone();

    // Per-session PID-file key (tracking_key + session.id) so two
    // consecutive sessions for the same stage never share a PID file (O-14).
    let pid_key = format!("{}-{}", title, session.id);

    // Build the kind-specific initial prompt.
    let signal_path_str = signal_path.to_string_lossy();
    let initial_prompt = match kind {
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
    };
    let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));

    // Model/effort POLICY (kept explicit, not buried). Merge and
    // base-conflict resolution always run on the strongest model with
    // maximum deliberation regardless of the originating stage's settings;
    // stage and knowledge sessions use the stage's effective values.
    let (model, effort) = match kind {
        SessionType::Merge | SessionType::BaseConflict => ("opus", "xhigh"),
        SessionType::Stage | SessionType::Knowledge => {
            (stage.effective_model(), stage.effective_reasoning_effort())
        }
    };

    // Resolve the Claude Code permission mode and pass it on the CLI. Loom
    // stages run autonomously with no human at the terminal, so they must
    // START in the resolved mode (default: `auto`). Writing
    // `permissions.defaultMode` into the worktree's settings.local.json is
    // NOT sufficient: Claude Code v2.1.142+ ignores `defaultMode: "auto"`
    // from project/local settings files, so only `--permission-mode` is
    // honored (see build_claude_command). Resolved from the same
    // `[plan_sandbox]` snapshot the settings generator reads
    // (OrchestratorConfig.sandbox_config is loaded from it too), so the CLI
    // flag and the generated settings file never disagree.
    let permission_mode = {
        let plan_sandbox = crate::fs::work_dir::read_plan_sandbox(work_dir)
            .ok()
            .flatten()
            .unwrap_or_default();
        crate::sandbox::merge_config(
            &plan_sandbox,
            &stage.sandbox,
            stage.stage_type,
            &stage.implementers,
        )
        .permission_mode
    };

    // Find claude's absolute path (needed for macOS where terminals don't inherit PATH).
    // build_claude_command shell-escapes the path, model, effort, and mode (S-3).
    let claude_path = find_claude_path()?;
    let rc_name = remote_control_session_name(kind, stage);
    let remote_control = crate::remote_control::resolve_invocation(work_dir, &rc_name);
    let capsule = super::session_capsule(&claude_path, cwd);
    let claude_cmd = super::build_claude_command(
        &claude_path.display().to_string(),
        model,
        effort,
        permission_mode.as_settings_value(),
        &capsule,
        &remote_control,
        &escaped_prompt,
    );

    // Create the wrapper script (writes PID + start-time before exec'ing
    // claude). `stage.id` sets LOOM_STAGE_ID; `pid_key` names the per-session
    // PID file. Pass cwd so the script can cd there (macOS).
    let wrapper_path = super::wrapper::create_wrapper_script(
        work_dir,
        &pid_key,
        &stage.id,
        &session.id,
        &claude_cmd,
        Some(cwd),
        kind,
    )?;

    // Build the command that runs the wrapper script.
    // IMPORTANT: Use absolute path because macOS terminals open in home directory.
    let wrapper_path_abs = wrapper_path.canonicalize().unwrap_or(wrapper_path);

    Ok((session, title, pid_key, wrapper_path_abs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage_named(id: &str, name: &str) -> Stage {
        Stage {
            id: id.to_string(),
            name: name.to_string(),
            ..Stage::default()
        }
    }

    #[test]
    fn remote_control_session_name_stage_uses_bare_name() {
        let stage = stage_named("my-stage", "My Stage");
        assert_eq!(
            remote_control_session_name(SessionType::Stage, &stage),
            "My Stage"
        );
    }

    #[test]
    fn remote_control_session_name_merge_is_prefixed() {
        let stage = stage_named("my-stage", "My Stage");
        assert_eq!(
            remote_control_session_name(SessionType::Merge, &stage),
            "Merge: My Stage"
        );
    }

    #[test]
    fn remote_control_session_name_base_conflict_is_prefixed() {
        let stage = stage_named("my-stage", "My Stage");
        assert_eq!(
            remote_control_session_name(SessionType::BaseConflict, &stage),
            "Base conflict: My Stage"
        );
    }

    #[test]
    fn remote_control_session_name_knowledge_is_prefixed() {
        let stage = stage_named("my-stage", "My Stage");
        assert_eq!(
            remote_control_session_name(SessionType::Knowledge, &stage),
            "Knowledge: My Stage"
        );
    }

    #[test]
    fn remote_control_session_name_falls_back_to_stage_id_when_name_empty() {
        let stage = stage_named("my-stage", "   ");
        assert_eq!(
            remote_control_session_name(SessionType::Stage, &stage),
            "my-stage"
        );
    }

    #[test]
    fn remote_control_session_name_handles_fully_empty_stage() {
        let stage = Stage::default();
        assert_eq!(
            remote_control_session_name(SessionType::Merge, &stage),
            "Merge: "
        );
    }
}
