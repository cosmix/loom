//! `loom container logs` — tail or follow a running stage's container logs.
//!
//! Mirrors the session-lookup pattern from `commands::container::shell`:
//! scan `.work/sessions/*.md` for a session with a matching `stage_id` and
//! a populated `container_name`, then exec into `<runtime> logs ...`.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::os::unix::process::CommandExt;
use std::process::Command;

use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::orchestrator::terminal::container::runtime as rt;
use crate::parser::frontmatter::parse_from_markdown;

pub fn execute(stage_id: String, follow: bool, tail: usize) -> Result<()> {
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
        found.ok_or_else(|| anyhow!("No container session found for stage {stage_id}"))?;
    let container_name = session
        .container_name
        .as_ref()
        .ok_or_else(|| anyhow!("Session has no container_name"))?;

    // Prefer the runtime persisted on the session (so detached daemons read
    // the same binary that spawned the container). Fall back to auto-detection
    // when the field is missing (legacy sessions).
    let runtime = session
        .runtime
        .as_deref()
        .and_then(rt::Runtime::from_binary)
        .map(Ok)
        .unwrap_or_else(|| rt::detect_runtime("auto"))?;

    let tail_arg = format!("--tail={tail}");
    let mut args: Vec<&str> = vec!["logs"];
    if follow {
        args.push("-f");
    }
    args.push(&tail_arg);
    args.push(container_name);

    // exec replaces this process so Ctrl-C, stdout buffering, and signal
    // handling behave like running `docker logs` directly.
    let err = Command::new(runtime.binary()).args(&args).exec();
    Err(anyhow!("Failed to exec {} logs: {err}", runtime.binary()))
}
