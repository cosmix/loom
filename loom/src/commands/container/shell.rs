//! `loom container shell` — open an interactive shell inside a running
//! stage's container.
//!
//! Each stage runs in its own container, removed on completion / kill / stop / clean.

use anyhow::{anyhow, Result};
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::commands::container::logs::resolve_session_for_shell;
use crate::fs::work_dir::WorkDir;

pub fn execute(stage_id: String) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let target = resolve_session_for_shell(&work_dir.sessions_dir(), &stage_id)?;

    // exec replaces this process; user gets an interactive shell.
    let err = Command::new(target.runtime.binary())
        .args(["exec", "-it", &target.container_name, "/bin/bash"])
        .exec();
    Err(anyhow!(
        "Failed to exec {} exec: {err}",
        target.runtime.binary()
    ))
}
