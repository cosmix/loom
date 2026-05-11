//! `loom container logs` — tail or follow a running stage's container logs.
//!
//! Mirrors the session-lookup pattern from `commands::container::shell`:
//! scan `.work/sessions/*.md` for a session with a matching `stage_id` and
//! a populated `container_name`, then exec into `<runtime> logs ...`.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

use crate::fs::work_dir::WorkDir;
use crate::models::session::Session;
use crate::orchestrator::terminal::container::runtime as rt;
use crate::parser::frontmatter::parse_from_markdown;

/// Resolved session lookup: the container name and runtime binary to attach to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ResolvedTarget {
    pub container_name: String,
    pub runtime: rt::Runtime,
}

/// Find a container-backed session for `stage_id` by scanning
/// `sessions_dir` for `.md` files. Returns the first match that has both
/// a `stage_id` equal to the argument and a populated `container_name`.
///
/// The runtime is taken from the session's persisted `runtime` field when
/// present (so detached daemons read the same binary that spawned the
/// container); otherwise it falls back to auto-detection.
pub(crate) fn resolve_session_for_stage(
    sessions_dir: &Path,
    stage_id: &str,
) -> Result<ResolvedTarget> {
    if !sessions_dir.exists() {
        return Err(anyhow!(
            "No sessions directory at {}",
            sessions_dir.display()
        ));
    }

    let mut found: Option<Session> = None;
    for entry in fs::read_dir(sessions_dir)
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
        if session.stage_id.as_deref() == Some(stage_id) && session.container_name.is_some() {
            found = Some(session);
            break;
        }
    }

    let session =
        found.ok_or_else(|| anyhow!("No container session found for stage {stage_id}"))?;
    let container_name = session
        .container_name
        .clone()
        .ok_or_else(|| anyhow!("Session has no container_name"))?;

    let runtime = session
        .runtime
        .as_deref()
        .and_then(rt::Runtime::from_binary)
        .map(Ok)
        .unwrap_or_else(|| rt::detect_runtime("auto"))?;

    Ok(ResolvedTarget {
        container_name,
        runtime,
    })
}

pub fn execute(stage_id: String, follow: bool, tail: usize) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let target = resolve_session_for_stage(&work_dir.sessions_dir(), &stage_id)?;

    let tail_arg = format!("--tail={tail}");
    let mut args: Vec<&str> = vec!["logs"];
    if follow {
        args.push("-f");
    }
    args.push(&tail_arg);
    args.push(&target.container_name);

    // exec replaces this process so Ctrl-C, stdout buffering, and signal
    // handling behave like running `docker logs` directly.
    let err = Command::new(target.runtime.binary()).args(&args).exec();
    Err(anyhow!(
        "Failed to exec {} logs: {err}",
        target.runtime.binary()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_session(dir: &Path, filename: &str, body: &str) {
        std::fs::write(dir.join(filename), body).unwrap();
    }

    fn session_md(
        id: &str,
        stage_id: Option<&str>,
        runtime: Option<&str>,
        container_name: Option<&str>,
    ) -> String {
        let mut s = String::from("---\n");
        s.push_str(&format!("id: {id}\n"));
        match stage_id {
            Some(v) => s.push_str(&format!("stage_id: {v}\n")),
            None => s.push_str("stage_id: null\n"),
        }
        s.push_str("status: running\n");
        s.push_str("context_tokens: 0\n");
        s.push_str("context_limit: 200000\n");
        s.push_str("created_at: 2026-05-11T00:00:00Z\n");
        s.push_str("last_active: 2026-05-11T00:00:00Z\n");
        s.push_str("backend: container\n");
        if let Some(rt) = runtime {
            s.push_str(&format!("runtime: {rt}\n"));
        }
        if let Some(cn) = container_name {
            s.push_str(&format!("container_name: {cn}\n"));
        }
        s.push_str("---\n# Session\n");
        s
    }

    #[test]
    fn resolves_first_matching_container_session() {
        let tmp = TempDir::new().unwrap();
        let sessions = tmp.path();

        write_session(
            sessions,
            "session-aaaa.md",
            &session_md(
                "session-aaaa",
                Some("other-stage"),
                Some("docker"),
                Some("loom-other-aaaa"),
            ),
        );
        write_session(
            sessions,
            "session-bbbb.md",
            &session_md(
                "session-bbbb",
                Some("my-stage"),
                Some("docker"),
                Some("loom-my-stage-bbbb"),
            ),
        );

        let target = resolve_session_for_stage(sessions, "my-stage").unwrap();
        assert_eq!(target.container_name, "loom-my-stage-bbbb");
        assert_eq!(target.runtime.binary(), "docker");
    }

    #[test]
    fn returns_error_when_no_session_matches() {
        let tmp = TempDir::new().unwrap();
        let sessions = tmp.path();

        write_session(
            sessions,
            "session-aaaa.md",
            &session_md(
                "session-aaaa",
                Some("other-stage"),
                Some("docker"),
                Some("loom-other-aaaa"),
            ),
        );

        let err = resolve_session_for_stage(sessions, "missing-stage").unwrap_err();
        assert!(
            err.to_string().contains("No container session found"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn skips_sessions_without_container_name() {
        let tmp = TempDir::new().unwrap();
        let sessions = tmp.path();

        write_session(
            sessions,
            "session-native.md",
            &session_md("session-native", Some("my-stage"), None, None),
        );
        write_session(
            sessions,
            "session-cont.md",
            &session_md(
                "session-cont",
                Some("my-stage"),
                Some("podman"),
                Some("loom-my-stage-cont"),
            ),
        );

        let target = resolve_session_for_stage(sessions, "my-stage").unwrap();
        assert_eq!(target.container_name, "loom-my-stage-cont");
        assert_eq!(target.runtime.binary(), "podman");
    }

    #[test]
    fn errors_when_sessions_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let err = resolve_session_for_stage(&missing, "any").unwrap_err();
        assert!(
            err.to_string().contains("No sessions directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn ignores_malformed_session_files() {
        let tmp = TempDir::new().unwrap();
        let sessions = tmp.path();

        std::fs::write(sessions.join("garbage.md"), "not yaml at all").unwrap();
        write_session(
            sessions,
            "session-good.md",
            &session_md(
                "session-good",
                Some("my-stage"),
                Some("docker"),
                Some("loom-my-stage-good"),
            ),
        );

        let target = resolve_session_for_stage(sessions, "my-stage").unwrap();
        assert_eq!(target.container_name, "loom-my-stage-good");
    }
}
