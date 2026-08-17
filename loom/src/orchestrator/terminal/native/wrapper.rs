//! Authoring of the per-session wrapper script that `exec`s claude.
//!
//! Split out of `pid_tracking`, which owns PID files and process discovery.
//! This module owns the other half of the contract: the exact environment a
//! stage session is born with. That environment is a security boundary (the
//! script rebuilds it from an allowlist rather than inheriting the host's) and
//! a correctness boundary (`LOOM_STAGE_ID` and `LOOM_WORKTREE_PATH` are read by
//! hooks, the CLI and the daemon), so it is worth reading on its own.

use super::pid_tracking::{
    create_pid_dir, create_wrappers_dir, pid_file_path, wrapper_script_path,
};
use crate::models::session::SessionType;
use anyhow::{Context, Result};
use shell_escape::escape;
use std::fs;
use std::path::{Path, PathBuf};

/// Line continuation inside the generated `exec env -i …` invocation.
const CONTINUATION: &str = "\\\n";

/// Create a wrapper script that writes its PID before exec'ing claude.
///
/// The wrapper script:
/// 1. Changes to the working directory (important for macOS where terminals
///    can't reliably set cwd before spawning)
/// 2. Writes its own PID (`$$`) — and, on Linux, its start-time — to the PID
///    file, so liveness probes can detect PID reuse
/// 3. `exec`s the claude command under a rebuilt, allowlisted environment
///
/// # Arguments
/// * `work_dir` - The .work directory path
/// * `pid_key` - The per-session tracking key naming the PID file / wrapper
///   script (the session's stage-key + `session.id`). Distinct from `stage_id`
///   so two consecutive sessions for the same stage never share a PID file.
/// * `stage_id` - The value exported as `LOOM_STAGE_ID`. Always the plain plan
///   stage id, for every session kind. The kind-prefixed form (`merge-…`,
///   `knowledge-…`, `base-conflict-…`) names OS resources only — it must never
///   reach the environment, because every consumer (`loom memory`,
///   `loom handoff`, the heartbeat files, `session-end.sh`'s stage-file glob)
///   looks the value up as a real stage id.
/// * `session_id` - The session identifier (for LOOM_SESSION_ID env var)
/// * `claude_cmd` - The claude command to execute (e.g., "claude 'prompt here'")
/// * `working_dir` - The working directory to cd into before running claude
/// * `kind` - The session kind. Drives the two env vars that are NOT derivable
///   from `stage_id`; see `kind_env`.
///
/// # Returns
/// The path to the created wrapper script
pub fn create_wrapper_script(
    work_dir: &Path,
    pid_key: &str,
    stage_id: &str,
    session_id: &str,
    claude_cmd: &str,
    working_dir: Option<&Path>,
    kind: SessionType,
) -> Result<PathBuf> {
    create_wrappers_dir(work_dir)?;
    create_pid_dir(work_dir)?;

    let wrapper_path = wrapper_script_path(work_dir, pid_key);
    let script = build_wrapper_script(
        work_dir,
        &pid_file_path(work_dir, pid_key),
        stage_id,
        session_id,
        claude_cmd,
        working_dir,
        kind,
    );

    fs::write(&wrapper_path, &script)
        .with_context(|| format!("Failed to write wrapper script: {}", wrapper_path.display()))?;

    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wrapper_path)?.permissions();
        // Owner-only execute: wrapper scripts are run by the same user, no need for
        // group/other execute permissions. This prevents other users from reading
        // or executing the script, which contains session IDs and paths.
        perms.set_mode(0o700);
        fs::set_permissions(&wrapper_path, perms)?;
    }

    Ok(wrapper_path)
}

/// Absolute form of `path`, falling back to the input when it cannot be
/// resolved. Paths are absolutized because the script may `cd` elsewhere.
///
/// Shared with `native::capsule`: the `--settings` path it resolves and the
/// `cd` target built here must resolve to the same root, or the wrapper's
/// `cd` moves the process to a directory the `--settings` path was never
/// made relative to.
pub(super) fn absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Absolute form of a file that does not exist yet: resolve the parent and
/// re-attach the file name.
fn absolute_target(path: &Path) -> PathBuf {
    if path.exists() {
        return absolute(path);
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    }
}

/// The `cd` preamble, empty when the session has no working directory.
fn cd_section(working_dir: Option<&Path>) -> String {
    let Some(dir) = working_dir else {
        return String::new();
    };
    let dir_escaped = escape(absolute(dir).display().to_string().into());
    format!(
        r#"# Change to working directory
cd {dir_escaped} || {{ echo "Failed to cd to working directory"; exit 1; }}

"#,
    )
}

/// The two env assignments that depend on the session KIND rather than on any
/// id, returned as `(merge_session_env, worktree_path_env)`.
///
/// * `LOOM_MERGE_SESSION` — only `Merge`, which `commit-guard.sh` lets exit
///   without a commit. Keyed on the kind, never on a `merge-` prefix in
///   `stage_id`: that id is the plain plan stage id for every kind, and a plan
///   is free to name a stage `merge-anything`.
/// * `LOOM_WORKTREE_PATH` — only `Stage`, the one kind that runs inside a loom
///   worktree. Merge, knowledge and base-conflict sessions `cd` into the main
///   repo; exporting the var for them makes presence-based gates
///   (`sandbox_control_session`, `loom-control-complete.sh`) misread a
///   main-repo agent as a sandboxed worktree agent, which is what once made
///   knowledge stages impossible to complete.
fn kind_env(kind: SessionType, working_dir: Option<&Path>) -> (String, String) {
    let merge = if kind == SessionType::Merge {
        format!("    LOOM_MERGE_SESSION=1 {CONTINUATION}")
    } else {
        String::new()
    };
    let worktree = match working_dir.filter(|_| kind == SessionType::Stage) {
        Some(dir) => {
            let assignment = format!("LOOM_WORKTREE_PATH={}", absolute(dir).display());
            format!("    {} {CONTINUATION}", escape(assignment.into()))
        }
        None => String::new(),
    };
    (merge, worktree)
}

/// Records the PID and, best-effort on Linux, the process start-time on line 2
/// so liveness probes can detect PID reuse. `exec` preserves both, so they
/// identify the claude process after it replaces this shell.
fn pid_capture(pid_file: &str) -> String {
    format!(
        r#"# Write our PID, then (best-effort, Linux) the process start-time on
# line 2 so liveness probes can detect PID reuse. exec preserves the PID and
# start-time, so these identify the claude process after exec replaces us.
echo $$ > {pid_file}
if [ -r "/proc/$$/stat" ]; then
    # Field 22 of /proc/<pid>/stat is starttime. The comm field (2) is wrapped
    # in parens and may contain spaces, so strip through the last ')' first.
    _loom_stat=$(cat "/proc/$$/stat" 2>/dev/null)
    _loom_after=${{_loom_stat##*) }}
    _loom_start=$(echo "$_loom_after" | awk '{{print $20}}')
    if [ -n "$_loom_start" ]; then
        echo "$_loom_start" >> {pid_file}
    fi
fi
"#
    )
}

/// Rebuilds the child environment from a minimal host allowlist rather than
/// inheriting it, so ambient credentials and token-shaped variables never
/// reach a stage session. Fully static — no interpolation.
const ENV_ALLOWLIST: &str = r#"# Reconstruct the stage environment from a minimal host allowlist. In
# particular, ambient credentials and token-shaped variables are not inherited.
_loom_env=(
    "HOME=${HOME:-}"
    "PATH=${PATH:-/usr/bin:/bin}"
)
for _loom_name in LANG LC_ALL LC_CTYPE TERM COLORTERM TERM_PROGRAM SHELL DISPLAY \
    WAYLAND_DISPLAY XAUTHORITY DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR \
    TMUX_TMPDIR TMUX TMUX_PANE TMPDIR; do
    _loom_value="${!_loom_name}"
    if [ -n "$_loom_value" ]; then
        _loom_env+=("$_loom_name=$_loom_value")
    fi
done
"#;

/// Render the wrapper script text. Pure: every path is resolved by the caller
/// or by `absolute*`, and nothing is written.
fn build_wrapper_script(
    work_dir: &Path,
    host_pid_file: &Path,
    stage_id: &str,
    session_id: &str,
    claude_cmd: &str,
    working_dir: Option<&Path>,
    kind: SessionType,
) -> String {
    let cd_section = cd_section(working_dir);
    let (merge_session_env, worktree_path_env) = kind_env(kind, working_dir);

    // Shell-escape complete NAME=value words so quotes never nest incorrectly.
    let session_env = escape(format!("LOOM_SESSION_ID={session_id}").into());
    let stage_env = escape(format!("LOOM_STAGE_ID={stage_id}").into());
    let work_dir_env = escape(format!("LOOM_WORK_DIR={}", absolute(work_dir).display()).into());
    let pid_file = escape(absolute_target(host_pid_file).display().to_string().into());
    let pid_capture = pid_capture(&pid_file);

    format!(
        r#"#!/bin/bash
# Loom stage wrapper
# Writes PID to file before exec'ing claude

{cd_section}{pid_capture}
{ENV_ALLOWLIST}
# Loom stages record knowledge through `loom memory` / `loom knowledge`; Claude
# Code auto-memory writes to a location invisible to orchestration, so disable
# it at the process boundary rather than by instruction alone.
# Replace this process with claude under only the explicit stage contract.
exec env -i "${{_loom_env[@]}}" \
    {session_env} \
    {stage_env} \
    {work_dir_env} \
    "LOOM_MAIN_AGENT_PID=$$" \
    "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1" \
    "CLAUDE_CODE_DISABLE_AUTO_MEMORY=1" \
    "CLAUDE_REMOTE_CONTROL_SESSION_NAME_PREFIX=loom" \
{merge_session_env}{worktree_path_env}    {claude_cmd}
"#
    )
}

#[cfg(test)]
mod tests;
