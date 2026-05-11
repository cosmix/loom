//! `loom container shell` — open an interactive shell inside a running
//! stage's container.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::orchestrator::terminal::container::runtime as rt;
use crate::parser::frontmatter::parse_from_markdown;

pub fn execute(stage_id: String) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let sessions_dir = work_dir.sessions_dir();
    if !sessions_dir.exists() {
        return Err(anyhow!(
            "No sessions directory at {}",
            sessions_dir.display()
        ));
    }

    let mut found: Option<Session> = None;
    for entry in fs::read_dir(&sessions_dir)
        .with_context(|| format!("Failed to read sessions dir {}", sessions_dir.display()))?
        .flatten()
    {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let session: Session = match parse_from_markdown(&content, "Session") {
            Ok(s) => s,
            Err(_) => continue,
        };
        if session.stage_id.as_deref() == Some(stage_id.as_str())
            && session.container_name.is_some()
        {
            found = Some(session);
            break;
        }
    }

    let session =
        found.ok_or_else(|| anyhow!("No live container session found for stage {stage_id}"))?;
    let container_name = session
        .container_name
        .as_ref()
        .ok_or_else(|| anyhow!("Session has no container_name"))?;

    let runtime = rt::detect_runtime("auto")?;

    // exec replaces this process; user gets an interactive shell.
    let err = Command::new(runtime.binary())
        .args(["exec", "-it", container_name, "/bin/bash"])
        .exec();
    Err(anyhow!("Failed to exec {} exec: {err}", runtime.binary()))
}
