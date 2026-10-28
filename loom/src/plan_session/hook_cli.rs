//! Entry of `loom plan hook`: routes a Claude Code hook event of a planning session.

use std::io::{Read, Write};
use std::path::Path;

use super::sandbox::PRE_TOOL_USE;

/// Write the hook answer, if any, and always return a successful hook exit code.
pub fn run(event: &str, session_dir: &Path, stdin: impl Read, mut stdout: impl Write) -> i32 {
    if event == PRE_TOOL_USE {
        if let Some(answer) = super::guard::run_pre_tool_use(session_dir, stdin) {
            let _ = writeln!(stdout, "{answer}");
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
