//! Entry of `loom plan hook`: routes a Claude Code hook event of a planning session.

use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::bridge::host::socket_name;
use super::bridge::registry::read_record;
use super::kind::SessionDir;
use super::owner_watch::SessionOwnerProbe;
use super::sandbox::PRE_TOOL_USE;
use super::spool::{append_event, read_payload};
use crate::planning::runtime::runtime_paths_under;

/// Write the hook answer, if any, and always return a successful hook exit code.
pub fn run(event: &str, session_dir: &Path, stdin: impl Read, mut stdout: impl Write) -> i32 {
    match event {
        PRE_TOOL_USE => {
            if let Some(answer) = super::guard::run_pre_tool_use(session_dir, stdin) {
                let _ = writeln!(stdout, "{answer}");
            }
        }
        "PermissionRequest" => super::permission::run_hook(session_dir, stdin, stdout),
        "OwnerWatch" => owner_watch(session_dir),
        "SessionStart" | "UserPromptSubmit" | "Notification" | "Stop" | "StopFailure"
        | "SubagentStart" | "SubagentStop" | "PreCompact" | "SessionEnd" | "PermissionDenied" => {
            // A spool event prints nothing whatever happens: the bridge reads the spool, and a
            // panic is no reason to fail Claude Code's hook.
            let _ = catch_unwind(AssertUnwindSafe(|| spool(event, session_dir, stdin)));
        }
        _ => {}
    }
    0
}

fn owner_watch(session_dir: &Path) {
    if let Some((lock, socket)) = watch_target(session_dir) {
        // The launch script starts the watch in the pane's process group, which the pane leader's
        // exit and a tmux kill-server both signal. It must outlive them: clearing the tmux server
        // of a dead leader is its job. The only failure is a process already leading a group,
        // which a background job of the script never is, and the watch then runs where it is.
        let _ = nix::unistd::setsid();
        SessionOwnerProbe::new(&lock, &socket).watch();
    }
}

/// The server lock and tmux socket to watch, only when the session record and the guard policy
/// agree on the session. As in recovery, a planted record must not steer teardown to another
/// session's socket.
fn watch_target(session_dir: &Path) -> Option<(PathBuf, String)> {
    let record = read_record(&SessionDir::new(session_dir.to_owned())).ok()?;
    let policy = super::guard::load_policy(session_dir).ok()?;
    // The host stores runtime state at <loom user dir>/plan/<project key>.
    let loom_dir = policy.runtime_dir.parent().and_then(Path::parent)?;
    let runtime = runtime_paths_under(loom_dir, &policy.project_root).ok()?;
    let agrees = runtime.dir == policy.runtime_dir
        && runtime.sessions.join(&record.id) == session_dir
        && record.socket == socket_name(&record.id);
    agrees.then_some((runtime.lock, record.socket))
}

/// Append the payload to the spool, unless it belongs to another event (the spool lesson,
/// `mistakes/spurious-waiting-for-input.md`) or cannot be read; every error is dropped.
fn spool(event: &str, session_dir: &Path, stdin: impl Read) {
    let Some(payload) = read_payload(stdin) else {
        return;
    };
    match payload.get("hook_event_name") {
        None | Some(Value::Null) => {}
        Some(Value::String(name)) if name == event => {}
        Some(_) => return,
    }
    let _ = append_event(session_dir, event, &payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_session::bridge::registry::{write_record, SessionRecord};
    use crate::plan_session::guard::GuardPolicy;
    use crate::plan_session::kind::SessionKind;
    use serde_json::json;

    const SPOOL: [&str; 10] = [
        "SessionStart",
        "UserPromptSubmit",
        "Notification",
        "Stop",
        "StopFailure",
        "SubagentStart",
        "SubagentStop",
        "PreCompact",
        "SessionEnd",
        "PermissionDenied",
    ];

    fn session() -> (tempfile::TempDir, std::path::PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap().join("s1");
        std::fs::create_dir(&dir).unwrap();
        (root, dir)
    }

    fn spool_lines(dir: &Path) -> Vec<Value> {
        let text = std::fs::read_to_string(dir.join("events.jsonl")).unwrap_or_default();
        text.lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn hook(event: &str, dir: &Path, stdin: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        assert_eq!(run(event, dir, stdin, &mut output), 0, "{event}");
        output
    }

    #[test]
    fn unknown_event_is_silent_and_succeeds() {
        let mut output = Vec::new();
        let code = run("Unknown", Path::new("missing"), &b""[..], &mut output);

        assert_eq!(code, 0);
        assert!(output.is_empty());
    }

    #[test]
    fn permission_denied_spools_without_a_retry_response() {
        let (_root, dir) = session();
        let payload = json!({
            "hook_event_name": "PermissionDenied",
            "tool_name": "Bash",
            "reason": "[Irreversible Local Destruction]",
        });

        let output = hook("PermissionDenied", &dir, payload.to_string().as_bytes());

        assert!(output.is_empty());
        let lines = spool_lines(&dir);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["event"], "PermissionDenied");
        assert_eq!(lines[0]["payload"], payload);
    }

    #[test]
    fn owner_watch_without_a_record_returns_silently_without_blocking() {
        let (_root, dir) = session();
        let (send, receive) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            send.send(hook("OwnerWatch", &dir, b"")).unwrap();
        });

        let output = receive
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("OwnerWatch must return immediately without a session record");

        assert!(output.is_empty());
        worker.join().unwrap();
    }

    #[test]
    fn owner_watch_with_an_unreadable_record_is_silent_and_succeeds() {
        let (_root, dir) = session();
        std::fs::write(dir.join("session.json"), b"{invalid json").unwrap();

        assert!(hook("OwnerWatch", &dir, b"").is_empty());
    }

    const ID: &str = "0f8e7d6c-5b4a-4938-8271-605f4e3d2c1b";

    /// A session directory under a real runtime directory, with a record and a guard policy
    /// that agree; `tweak` edits both before they are written.
    fn watched(
        tweak: impl FnOnce(&mut SessionRecord, &mut GuardPolicy),
    ) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let project = base.join("proj");
        std::fs::create_dir(&project).unwrap();
        let runtime = runtime_paths_under(&base.join("loom"), &project).unwrap();
        let dir = runtime.sessions.join(ID);
        std::fs::create_dir_all(&dir).unwrap();
        let mut policy = GuardPolicy {
            schema: 1,
            kind: SessionKind::Authoring,
            project_root: project,
            runtime_dir: runtime.dir,
            session_dir: dir.clone(),
            home: base.join("home"),
        };
        let mut record = SessionRecord {
            schema: 1,
            id: ID.to_owned(),
            kind: SessionKind::Authoring,
            socket: socket_name(ID),
            tmux_session: "loom".to_owned(),
            pid: None,
            start_time: None,
            transcript_path: None,
            started_at: "2026-01-01T00:00:00Z".to_owned(),
            feature: None,
            plan: None,
            children: Vec::new(),
        };
        tweak(&mut record, &mut policy);
        write_record(&SessionDir::new(dir.clone()), &record).unwrap();
        std::fs::write(dir.join("guard.json"), serde_json::to_vec(&policy).unwrap()).unwrap();
        (tmp, dir, runtime.lock)
    }

    #[test]
    fn watch_target_of_a_consistent_session_is_its_lock_and_socket() {
        let (_tmp, dir, lock) = watched(|_, _| {});

        assert_eq!(watch_target(&dir), Some((lock, socket_name(ID))));
    }

    #[test]
    fn watch_target_refuses_a_record_naming_another_socket() {
        let (_tmp, dir, _) = watched(|record, _| record.socket = socket_name("another-session"));

        assert_eq!(watch_target(&dir), None);
    }

    #[test]
    fn watch_target_refuses_a_record_with_another_id() {
        // The socket matches the planted id, so only the session-directory check can refuse.
        let other = "ffffffff-eeee-4ddd-8ccc-bbbbbbbbbbbb";
        let (_tmp, dir, _) = watched(|record, _| {
            record.id = other.to_owned();
            record.socket = socket_name(other);
        });

        assert_eq!(watch_target(&dir), None);
    }

    #[test]
    fn watch_target_refuses_a_policy_with_another_runtime_directory() {
        let (_tmp, dir, _) = watched(|_, policy| {
            let planning_root = policy.runtime_dir.parent().unwrap().to_owned();
            policy.runtime_dir = planning_root.join("another-project-000000000000");
        });

        assert_eq!(watch_target(&dir), None);
    }

    #[test]
    fn missing_session_denies_on_one_line_and_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let mut output = Vec::new();
        let code = run(
            "PreToolUse",
            &root.path().join("missing"),
            &b"{}"[..],
            &mut output,
        );

        assert_eq!(code, 0);
        let text = std::str::from_utf8(&output).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.ends_with('\n'));
        let answer: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(answer["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(answer["hookSpecificOutput"]["permissionDecision"], "deny");
    }

    #[test]
    fn every_spool_event_appends_one_silent_line() {
        let (_root, dir) = session();
        for event in SPOOL {
            let payload = json!({"session_id": "s", "hook_event_name": event});
            assert!(hook(event, &dir, payload.to_string().as_bytes()).is_empty());
        }
        let names: Vec<Value> = spool_lines(&dir)
            .iter()
            .map(|line| line["event"].clone())
            .collect();
        assert_eq!(names, SPOOL.map(Value::from));
    }

    #[test]
    fn a_mismatched_event_name_appends_nothing() {
        let (_root, dir) = session();
        for name in [json!("SubagentStop"), json!(7)] {
            let payload = json!({"hook_event_name": name});
            assert!(hook("Stop", &dir, payload.to_string().as_bytes()).is_empty());
        }
        assert!(spool_lines(&dir).is_empty());
    }

    #[test]
    fn an_oversized_or_invalid_payload_appends_nothing() {
        let (_root, dir) = session();
        let huge = vec![b' '; 1024 * 1024 + 1];
        assert!(hook("Notification", &dir, &huge).is_empty());
        assert!(hook("Stop", &dir, b"{not json").is_empty());
        assert!(spool_lines(&dir).is_empty());
    }

    #[test]
    fn a_missing_session_directory_is_silent_for_every_spool_event() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("gone");
        for event in SPOOL {
            let payload = json!({"hook_event_name": event});
            assert!(hook(event, &missing, payload.to_string().as_bytes()).is_empty());
        }
        assert!(!missing.exists());
    }

    #[test]
    fn permission_request_reaches_the_permission_hook() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("gone");
        let payload = json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash"});
        let output = hook(
            "PermissionRequest",
            &missing,
            payload.to_string().as_bytes(),
        );
        let answer: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(
            answer["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(answer["hookSpecificOutput"]["decision"]["behavior"], "deny");
    }
}
