//! The `claude` command line and the session capsule for one launch.

use anyhow::Result;
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use super::super::SessionCapsule;
use super::host::LaunchHost;
use crate::models::session::SessionType;
use crate::models::stage::Stage;
use crate::sandbox::MergedSandboxConfig;

/// The `claude` invocation for this launch; `build_claude_command`
/// shell-escapes the path, model, effort, mode and remote-control name (S-3).
/// The claude path is absolute, since macOS terminals do not inherit PATH.
/// Returns the command and the effort it passes as `--effort`.
pub(super) fn claude_command(
    host: &LaunchHost,
    work_dir: &Path,
    kind: SessionType,
    stage: &Stage,
    signal_path: &Path,
    sandbox: &MergedSandboxConfig,
    capsule: &SessionCapsule,
) -> (String, String) {
    let (model, effort) = super::model_and_effort(kind, stage, work_dir);
    let prompt = super::initial_prompt(kind, stage, signal_path);
    let escaped_prompt = escape(Cow::Borrowed(&prompt));
    let rc_name = super::remote_control_session_name(kind, stage);
    let remote_control = crate::remote_control::resolve_invocation(work_dir, &rc_name);
    let command = super::super::build_claude_command(
        &host.claude_path.display().to_string(),
        &model,
        &effort,
        sandbox.permission_mode.as_settings_value(),
        capsule,
        &remote_control,
        &escaped_prompt,
    );
    (command, effort)
}

/// The session's scratch dir and its capsule, with the capsule settings file
/// written and isolation confirmed.
pub(super) fn session_capsule_for(
    host: &LaunchHost,
    work_dir: &Path,
    kind: SessionType,
    stage: &Stage,
    session_id: &str,
    sandbox: &MergedSandboxConfig,
    cwd: &Path,
) -> Result<(PathBuf, SessionCapsule)> {
    let scratch_dir = host.prepare_scratch(session_id)?;
    let settings_file = host.write_capsule(
        work_dir,
        kind,
        stage,
        session_id,
        sandbox,
        cwd,
        &scratch_dir,
    )?;
    let prefix_file = super::resolve_prompt_cache_split_prefix_file(work_dir, stage);
    let capsule = super::super::session_capsule(
        host.capsule_support,
        Some(settings_file.clone()),
        prefix_file,
    );
    capsule.require_isolation(&settings_file)?;
    Ok((scratch_dir, capsule))
}
