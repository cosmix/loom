//! Launch of a planning session: the private tmux server, the sandboxed claude process and its identity.

pub mod script;

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::fs::permissions::trust_project;
use crate::fs::safe_fs::{safe_open_dirfd, safe_write_with_mode_in_workdir};
use crate::orchestrator::terminal::tmux::{spawn_in_tmux, teardown_socket};
use crate::plan_session::bridge::host::{LaunchSpec, Spawned, TeardownFailed};
use crate::plan_session::sandbox::{write_session_sandbox, SandboxInputs};
use crate::plan_session::spool::SPOOL_EVENTS;
use crate::plan_session::tmux_io::pane_pid;
use crate::process::{agent_session_environment_from, process_start_time, ProcessIdentity};

/// `launch.sh` is the host's, readable and runnable by the user alone.
const LAUNCH_SCRIPT_MODE: libc::mode_t = 0o700;

/// Write the session's sandbox and `launch.sh`, start its tmux server and read the identity of
/// the pane's process. A failure once the server may exist tears the server down first, so an
/// error leaves nothing running (`SessionHost::spawn`) unless it is a [`TeardownFailed`].
pub fn launch(spec: &LaunchSpec) -> Result<Spawned> {
    let script = prepare(spec)?;
    match start(spec, &script) {
        Ok(identity) => Ok(Spawned { identity }),
        Err(error) => Err(abandon(&spec.socket, error)),
    }
}

/// The project root's trust entry in `<home>/.claude.json`, `settings.json` and `guard.json` with
/// the spool hooks registered, then `launch.sh` (0700); returns the script's path.
fn prepare(spec: &LaunchSpec) -> Result<PathBuf> {
    // Claude Code runs no hook, `SessionStart` included, while its folder-trust dialog is open,
    // and neither a `--settings` key nor a trusted parent directory skips it: only this entry
    // does. Without it the session never leaves Starting, so a failure here is a hard error.
    trust_project(&spec.home, &spec.project_root).with_context(|| {
        format!(
            "cannot record {} as trusted in {}: the session would wait at Claude Code's trust dialog",
            spec.project_root.display(),
            spec.home.join(".claude.json").display()
        )
    })?;
    let inputs = SandboxInputs {
        project_root: &spec.project_root,
        runtime: &spec.runtime,
        session_dir: &spec.dir,
        kind: &spec.kind,
        loom_bin: &spec.loom_bin,
        home: &spec.home,
    };
    let files = write_session_sandbox(&inputs, &SPOOL_EVENTS)
        .context("cannot write the planning session's sandbox")?;
    // The agent allowlist of this process's environment, then the start request's extras;
    // `launch_script` drops a stage identity and pins the planning variables after both, so an
    // extra cannot replace them (`env -i` leaves nothing else).
    let mut env = agent_session_environment_from(std::env::vars_os());
    let extra = spec.extra_env.iter();
    env.extend(extra.map(|(name, value)| (name.into(), value.into())));
    let text = script::launch_script(spec, &files.settings, &env);
    let path = spec.dir.launch_script();
    write_script(spec.dir.path(), &path, &text)?;
    Ok(path)
}

/// Write `text` to `path` inside the session directory, no-follow, mode 0700.
fn write_script(session: &Path, path: &Path, text: &str) -> Result<()> {
    let dirfd = safe_open_dirfd(session)
        .with_context(|| format!("cannot open the session directory {}", session.display()))?;
    let rel = path
        .strip_prefix(session)
        .with_context(|| format!("{} is not under {}", path.display(), session.display()))?;
    safe_write_with_mode_in_workdir(dirfd.as_raw_fd(), rel, text.as_bytes(), LAUNCH_SCRIPT_MODE)
        .with_context(|| format!("cannot write {}", path.display()))
}

/// Start the server running `script` in the project root, then identify the pane's process: a
/// pid whose start time cannot be read is no identity.
fn start(spec: &LaunchSpec, script: &Path) -> Result<ProcessIdentity> {
    spawn_in_tmux(&spec.socket, &spec.tmux_session, &spec.project_root, script)?;
    let pid = pane_pid(&spec.socket, &spec.tmux_session)?;
    let start_time = process_start_time(pid).with_context(|| {
        format!("cannot read the start time of the planning session's process {pid}")
    })?;
    Ok(ProcessIdentity {
        pid,
        start_time: Some(start_time),
    })
}

/// Tear down a half-started server (`mistakes/tmux-backend.md`). A failed teardown may leave
/// the server running: the error is then a [`TeardownFailed`], which the bridge answers by
/// keeping the session's directory and record instead of clearing them.
fn abandon(socket: &str, error: anyhow::Error) -> anyhow::Error {
    match teardown_socket(socket) {
        Ok(()) => error,
        Err(teardown) => {
            let server = format!("the tmux server on socket '{socket}'");
            let teardown = teardown.context(server);
            anyhow::Error::new(TeardownFailed { error, teardown })
        }
    }
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
