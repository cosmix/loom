//! Entry of `loom plan hook`: routes a Claude Code hook event of a planning session.

use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use serde_json::Value;

use super::sandbox::PRE_TOOL_USE;
use super::spool::{append_event, read_payload};

/// Write the hook answer, if any, and always return a successful hook exit code.
pub fn run(event: &str, session_dir: &Path, stdin: impl Read, mut stdout: impl Write) -> i32 {
    match event {
        PRE_TOOL_USE => {
            if let Some(answer) = super::guard::run_pre_tool_use(session_dir, stdin) {
                let _ = writeln!(stdout, "{answer}");
            }
        }
        "PermissionRequest" => super::permission::run_hook(session_dir, stdin, stdout),
        "SessionStart" | "UserPromptSubmit" | "Notification" | "Stop" | "SubagentStart"
        | "SubagentStop" | "PreCompact" | "SessionEnd" => {
            // A spool event prints nothing whatever happens: the bridge reads the spool, and a
            // panic is no reason to fail Claude Code's hook.
            let _ = catch_unwind(AssertUnwindSafe(|| spool(event, session_dir, stdin)));
        }
        _ => {}
    }
    0
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
    use serde_json::json;

    const SPOOL: [&str; 8] = [
        "SessionStart",
        "UserPromptSubmit",
        "Notification",
        "Stop",
        "SubagentStart",
        "SubagentStop",
        "PreCompact",
        "SessionEnd",
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
