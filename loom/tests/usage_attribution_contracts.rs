//! Contract: `loom usage` shows real transcripts in the Attribution section,
//! on the terminal and in the JSON report.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use serde_json::{json, Value};
use std::fs;
use std::path::Path;

fn assistant(minute: u32, id: &str, content: Value) -> String {
    json!({
        "type": "assistant",
        "timestamp": format!("2026-10-14T06:{minute:02}:00Z"),
        "sessionId": "s1",
        "message": {
            "id": id,
            "model": "claude-sonnet-5",
            "content": content,
            "usage": {
                "input_tokens": 10,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0,
                "output_tokens": 5
            }
        }
    })
    .to_string()
}

/// Four requests; the Read result and the attachment follow the second.
fn write_transcript(root: &Path) {
    let read = json!([{
        "type": "tool_use",
        "id": "t1",
        "name": "Read",
        "input": {"file_path": "loom/src/x.rs"}
    }]);
    let result = json!({
        "type": "user",
        "timestamp": "2026-10-14T06:03:00Z",
        "sessionId": "s1",
        "message": {"content": [{
            "type": "tool_result",
            "tool_use_id": "t1",
            "content": "x".repeat(400)
        }]}
    });
    let attachment = json!({
        "type": "attachment",
        "timestamp": "2026-10-14T06:04:00Z",
        "sessionId": "s1",
        "attachment": {"type": "hook_additional_context", "content": ["context"]},
        "rendered": [{"content": "y".repeat(200)}]
    });
    let text = json!([{"type": "text", "text": "ok"}]);
    let lines = [
        assistant(1, "m1", text.clone()),
        assistant(2, "m2", read),
        result.to_string(),
        attachment.to_string(),
        assistant(5, "m3", text.clone()),
        assistant(6, "m4", text),
    ];
    let project = root.join("-tmp-proj");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("s1.jsonl"), format!("{}\n", lines.join("\n"))).unwrap();
}

fn usage(root: &Path, extra: &[&str]) -> String {
    let output = helpers::loom_cmd()
        .current_dir(root)
        .args(["usage", "--all", "--since", "2000-01-01", "--claude-root"])
        .arg(root)
        .args(extra)
        .output()
        .expect("spawn loom usage");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "loom usage failed\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

#[test]
fn usage_command_renders_attribution_rows() {
    let temp = tempfile::tempdir().unwrap();
    write_transcript(temp.path());

    let text = usage(temp.path(), &[]);
    let heading = text
        .lines()
        .position(|line| line.trim_start().starts_with("Attribution"))
        .unwrap_or_else(|| panic!("no Attribution heading in:\n{text}"));
    let section = text.lines().skip(heading).collect::<Vec<_>>().join("\n");
    assert!(
        section.contains("read:source-other"),
        "no read:source-other row under Attribution:\n{section}"
    );
    assert!(
        section.contains("attach:hook_additional_context"),
        "no attach:hook_additional_context row under Attribution:\n{section}"
    );

    let report: Value = serde_json::from_str(&usage(temp.path(), &["--json"])).unwrap();
    let total = report["attribution"]["total_s2"]
        .as_f64()
        .expect("attribution.total_s2 in the JSON report");
    assert!(total > 0.0, "total_s2 was {total}");
}
