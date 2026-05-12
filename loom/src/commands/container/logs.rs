//! `loom container logs` — tail or follow a running stage's container logs.
//!
//! Each stage runs in its own container, removed on completion / kill / stop / clean.
//!
//! Mirrors the session-lookup pattern from `commands::container::shell`:
//! scan `.work/sessions/*.md` for a session with a matching `stage_id` and
//! a populated `container_name`, then exec into `<runtime> logs ...`.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::BufReader;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::commands::container::log_format::{FormatOptions, LogFormat};
use crate::fs::work_dir::WorkDir;
use crate::models::session::{BackendType, Session, SessionStatus};
use crate::orchestrator::terminal::container::runtime as rt;
use crate::parser::frontmatter::parse_from_markdown;

/// Resolved session lookup: the container name and runtime binary to attach to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ResolvedTarget {
    pub container_name: String,
    pub runtime: rt::Runtime,
}

/// The live/exited/missing state of a container.
#[derive(Debug)]
pub(crate) enum ContainerState {
    Running,
    Exited(String),
    Missing,
}

/// Load all sessions from `sessions_dir`, surfacing parse failures via
/// `eprintln!` instead of silently skipping them.
fn load_sessions(sessions_dir: &Path) -> Result<Vec<Session>> {
    let mut sessions = Vec::new();
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
            Err(e) => {
                eprintln!("Warning: failed to read session file {:?}: {e}", path);
                continue;
            }
        };
        match parse_from_markdown::<Session>(&content, "Session") {
            Ok(s) => sessions.push(s),
            Err(e) => {
                eprintln!("Warning: failed to parse session file {:?}: {e}", path);
            }
        }
    }
    Ok(sessions)
}

/// Pure selector: picks the best container-backed session for a stage.
///
/// When `require_running` is `true`, only sessions with `status == Running`
/// are considered; when `false`, any status is accepted (useful for post-crash
/// log access). Among multiple matches the session with the newest
/// `last_active` timestamp wins.
pub(crate) fn pick_container_session<'a>(
    sessions: &'a [Session],
    stage_id: &str,
    require_running: bool,
) -> Option<&'a Session> {
    sessions
        .iter()
        .filter(|s| {
            s.stage_id.as_deref() == Some(stage_id)
                && s.backend == BackendType::Container
                && s.container_name.is_some()
                && (!require_running || s.status == SessionStatus::Running)
        })
        .max_by_key(|s| s.last_active)
}

/// Verify the actual container state via the runtime.
///
/// Uses `<runtime> inspect -f '{{.State.Status}}' <container_name>`.
/// "no such" in stderr → `Missing`. "running" stdout → `Running`.
/// Other output → `Exited(status)`. Runtime invocation failures that are
/// NOT "no such" (daemon down, permission denied, …) are propagated as `Err`.
pub(crate) fn verify_container_state(
    runtime: rt::Runtime,
    container_name: &str,
) -> Result<ContainerState> {
    let output = Command::new(runtime.binary())
        .args(["inspect", "-f", "{{.State.Status}}", container_name])
        .output()
        .with_context(|| {
            format!(
                "Failed to invoke `{} inspect {container_name}`",
                runtime.binary()
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if stderr.contains("no such") {
            return Ok(ContainerState::Missing);
        }
        return Err(anyhow!(
            "`{} inspect {container_name}` failed: {}",
            runtime.binary(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if status == "running" {
        Ok(ContainerState::Running)
    } else {
        Ok(ContainerState::Exited(status))
    }
}

/// Resolve a container session for use by `logs` (accepts running OR exited).
///
/// Returns `Ok(ResolvedTarget)` when a container-backed session exists and
/// the container is running or exited. Returns an actionable error when the
/// container is missing or no matching session exists.
pub(crate) fn resolve_session_for_logs(
    sessions_dir: &Path,
    stage_id: &str,
) -> Result<ResolvedTarget> {
    if !sessions_dir.exists() {
        return Err(anyhow!(
            "No sessions directory at {}",
            sessions_dir.display()
        ));
    }

    let sessions = load_sessions(sessions_dir)?;

    let session = pick_container_session(&sessions, stage_id, false)
        .ok_or_else(|| anyhow!("No container session found for stage {stage_id}"))?;

    let container_name = session
        .container_name
        .clone()
        .expect("pick_container_session guarantees container_name.is_some()");

    let runtime = session
        .runtime
        .as_deref()
        .and_then(rt::Runtime::from_binary)
        .map(Ok)
        .unwrap_or_else(|| rt::detect_runtime("auto"))?;

    match verify_container_state(runtime, &container_name)? {
        ContainerState::Running | ContainerState::Exited(_) => {}
        ContainerState::Missing => {
            return Err(anyhow!(
                "Container {container_name} for stage {stage_id} has been removed.\n\
                 Check .work/crashes/ for captured logs from this session, or run \
                 'loom container list' to see what is currently running."
            ));
        }
    }

    Ok(ResolvedTarget {
        container_name,
        runtime,
    })
}

/// Resolve a container session for use by `shell` (requires running).
///
/// Returns `Ok(ResolvedTarget)` when the container is currently running.
pub(crate) fn resolve_session_for_shell(
    sessions_dir: &Path,
    stage_id: &str,
) -> Result<ResolvedTarget> {
    if !sessions_dir.exists() {
        return Err(anyhow!(
            "No sessions directory at {}",
            sessions_dir.display()
        ));
    }

    let sessions = load_sessions(sessions_dir)?;

    let session = pick_container_session(&sessions, stage_id, true)
        .ok_or_else(|| anyhow!("No live container session found for stage {stage_id}"))?;

    let container_name = session
        .container_name
        .clone()
        .expect("pick_container_session guarantees container_name.is_some()");

    let runtime = session
        .runtime
        .as_deref()
        .and_then(rt::Runtime::from_binary)
        .map(Ok)
        .unwrap_or_else(|| rt::detect_runtime("auto"))?;

    match verify_container_state(runtime, &container_name)? {
        ContainerState::Running => {}
        ContainerState::Exited(s) => {
            return Err(anyhow!(
                "Container {container_name} for stage {stage_id} is not running (status: {s}).\n\
                 Use 'loom container logs {stage_id}' to view captured output."
            ));
        }
        ContainerState::Missing => {
            return Err(anyhow!(
                "Container {container_name} for stage {stage_id} has been removed.\n\
                 Check .work/crashes/ for captured logs from this session, or run \
                 'loom container list' to see what is currently running."
            ));
        }
    }

    Ok(ResolvedTarget {
        container_name,
        runtime,
    })
}

pub fn execute(
    stage_id: String,
    follow: bool,
    tail: usize,
    format: LogFormat,
    show_thinking: bool,
    verbose: bool,
) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let target = resolve_session_for_logs(&work_dir.sessions_dir(), &stage_id)?;

    let tail_arg = format!("--tail={tail}");
    let mut args: Vec<&str> = vec!["logs"];
    if follow {
        args.push("-f");
    }
    args.push(&tail_arg);
    args.push(&target.container_name);

    match format {
        LogFormat::Json => {
            // Pass through raw bytes verbatim.
            let status = Command::new(target.runtime.binary())
                .args(&args)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .with_context(|| format!("Failed to spawn {} logs", target.runtime.binary()))?;
            std::process::exit(status.code().unwrap_or(1));
        }
        LogFormat::Human => {
            let mut child = Command::new(target.runtime.binary())
                .args(&args)
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .with_context(|| format!("Failed to spawn {} logs", target.runtime.binary()))?;

            let stdout = child.stdout.take().ok_or_else(|| {
                anyhow!(
                    "Failed to capture stdout from {} logs",
                    target.runtime.binary()
                )
            })?;
            let reader = BufReader::new(stdout);
            let opts = FormatOptions {
                show_thinking,
                verbose,
            };
            let mut out = std::io::stdout();
            crate::commands::container::log_format::format_stream(reader, &opts, &mut out)?;

            let status = child
                .wait()
                .with_context(|| format!("Failed to wait for {} logs", target.runtime.binary()))?;
            std::process::exit(status.code().unwrap_or(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use tempfile::TempDir;

    fn write_session(dir: &Path, filename: &str, body: &str) {
        std::fs::write(dir.join(filename), body).unwrap();
    }

    fn ts(offset_secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + offset_secs, 0)
            .single()
            .unwrap()
    }

    fn make_session(
        id: &str,
        stage_id: Option<&str>,
        backend: BackendType,
        status: SessionStatus,
        container_name: Option<&str>,
        last_active: DateTime<Utc>,
    ) -> Session {
        Session {
            id: id.to_string(),
            stage_id: stage_id.map(String::from),
            worktree_path: None,
            pid: None,
            status,
            context_tokens: 0,
            context_limit: 200_000,
            created_at: Utc::now(),
            last_active,
            session_type: crate::models::session::SessionType::Stage,
            merge_source_branch: None,
            merge_target_branch: None,
            backend,
            runtime: Some("docker".to_string()),
            container_name: container_name.map(String::from),
            tracking_key: String::new(),
        }
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

    // ---- pick_container_session tests ----

    #[test]
    fn pick_running_over_completed_when_require_running_true() {
        let running = make_session(
            "s1",
            Some("my-stage"),
            BackendType::Container,
            SessionStatus::Running,
            Some("loom-my-stage"),
            ts(100),
        );
        let completed = make_session(
            "s2",
            Some("my-stage"),
            BackendType::Container,
            SessionStatus::Completed,
            Some("loom-my-stage-old"),
            ts(200), // newer, but Completed
        );
        let sessions = vec![completed, running];
        let result = pick_container_session(&sessions, "my-stage", true);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "s1");
    }

    #[test]
    fn pick_newest_when_two_running_for_same_stage() {
        let older = make_session(
            "s-older",
            Some("my-stage"),
            BackendType::Container,
            SessionStatus::Running,
            Some("loom-my-stage-older"),
            ts(100),
        );
        let newer = make_session(
            "s-newer",
            Some("my-stage"),
            BackendType::Container,
            SessionStatus::Running,
            Some("loom-my-stage-newer"),
            ts(200),
        );
        let sessions = vec![older, newer];
        let result = pick_container_session(&sessions, "my-stage", true);
        assert_eq!(result.unwrap().id, "s-newer");
    }

    #[test]
    fn only_completed_returns_none_when_require_running() {
        let completed = make_session(
            "s1",
            Some("my-stage"),
            BackendType::Container,
            SessionStatus::Completed,
            Some("loom-my-stage"),
            ts(100),
        );
        let sessions = vec![completed];
        assert!(pick_container_session(&sessions, "my-stage", true).is_none());
    }

    #[test]
    fn only_completed_returns_some_when_not_require_running() {
        let completed = make_session(
            "s1",
            Some("my-stage"),
            BackendType::Container,
            SessionStatus::Completed,
            Some("loom-my-stage"),
            ts(100),
        );
        let sessions = vec![completed];
        let result = pick_container_session(&sessions, "my-stage", false);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, "s1");
    }

    #[test]
    fn non_container_backend_filtered_out() {
        let native = make_session(
            "s-native",
            Some("my-stage"),
            BackendType::Native,
            SessionStatus::Running,
            Some("loom-my-stage"),
            ts(100),
        );
        let sessions = vec![native];
        assert!(pick_container_session(&sessions, "my-stage", false).is_none());
    }

    // ---- resolve_session_for_logs / resolve_session_for_shell (file-based) tests ----

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

        // Only verifies session lookup without docker; skip verify_container_state part
        // by testing resolve_session_for_logs would find the session before hitting docker.
        // We test pick_container_session which is the pure part.
        let sessions_data: Vec<Session> = {
            let content = std::fs::read_to_string(sessions.join("session-bbbb.md")).unwrap();
            vec![parse_from_markdown::<Session>(&content, "Session").unwrap()]
        };
        let result = pick_container_session(&sessions_data, "my-stage", false);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().container_name.as_deref(),
            Some("loom-my-stage-bbbb")
        );
    }

    #[test]
    fn returns_error_when_no_session_matches() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        write_session(
            sessions_dir,
            "session-aaaa.md",
            &session_md(
                "session-aaaa",
                Some("other-stage"),
                Some("docker"),
                Some("loom-other-aaaa"),
            ),
        );

        // Test the pure pick_container_session path
        let content = std::fs::read_to_string(sessions_dir.join("session-aaaa.md")).unwrap();
        let session = parse_from_markdown::<Session>(&content, "Session").unwrap();
        let sessions = vec![session];
        assert!(pick_container_session(&sessions, "missing-stage", false).is_none());
    }

    #[test]
    fn skips_sessions_without_container_name() {
        let native_md = {
            let mut s = String::from("---\n");
            s.push_str("id: session-native\n");
            s.push_str("stage_id: my-stage\n");
            s.push_str("status: running\n");
            s.push_str("context_tokens: 0\n");
            s.push_str("context_limit: 200000\n");
            s.push_str("created_at: 2026-05-11T00:00:00Z\n");
            s.push_str("last_active: 2026-05-11T00:00:00Z\n");
            s.push_str("backend: native\n");
            s.push_str("---\n# Session\n");
            s
        };
        let cont_md = session_md(
            "session-cont",
            Some("my-stage"),
            Some("podman"),
            Some("loom-my-stage-cont"),
        );

        let native: Session = parse_from_markdown(&native_md, "Session").unwrap();
        let cont: Session = parse_from_markdown(&cont_md, "Session").unwrap();
        let sessions = vec![native, cont];

        let result = pick_container_session(&sessions, "my-stage", false);
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().container_name.as_deref(),
            Some("loom-my-stage-cont")
        );
    }

    #[test]
    fn errors_when_sessions_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let err = resolve_session_for_logs(&missing, "any").unwrap_err();
        assert!(
            err.to_string().contains("No sessions directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn ignores_malformed_session_files() {
        let tmp = TempDir::new().unwrap();
        let sessions_dir = tmp.path();

        std::fs::write(sessions_dir.join("garbage.md"), "not yaml at all").unwrap();
        write_session(
            sessions_dir,
            "session-good.md",
            &session_md(
                "session-good",
                Some("my-stage"),
                Some("docker"),
                Some("loom-my-stage-good"),
            ),
        );

        let content = std::fs::read_to_string(sessions_dir.join("session-good.md")).unwrap();
        let session = parse_from_markdown::<Session>(&content, "Session").unwrap();
        let sessions = vec![session];
        let result = pick_container_session(&sessions, "my-stage", false);
        assert_eq!(
            result.unwrap().container_name.as_deref(),
            Some("loom-my-stage-good")
        );
    }
}
