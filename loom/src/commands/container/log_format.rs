//! Stream-json (JSONL) log formatter for Claude Code container sessions.
//!
//! Parses the JSONL log output from `<runtime> logs` and renders it in a
//! human-readable form. Each line in the log stream is a JSON object with a
//! top-level `"type"` field.

use clap::ValueEnum;
use serde_json::Value;
use std::io::{BufRead, Write};

/// Output format for `loom container logs`.
#[derive(Clone, Debug, PartialEq, Eq, ValueEnum, Default)]
pub enum LogFormat {
    /// Human-readable rendering of stream-json events (default).
    #[default]
    Human,
    /// Raw JSON passthrough — bytes are copied verbatim.
    Json,
}

/// Options controlling human-readable rendering.
pub struct FormatOptions {
    /// Emit `[thinking]` prefixed lines for assistant thinking blocks.
    pub show_thinking: bool,
    /// Append a suppressed-events footer and count rate-limit / system events.
    pub verbose: bool,
}

/// Format a single stream-json line, writing rendered output to `out`.
///
/// On malformed JSON (or a line that does not parse as a JSON object),
/// writes `[malformed line: <first 40 chars>]\n` and returns `Ok(())`.
///
/// On IO errors writing to `out`, the error is propagated.
pub fn format_line(line: &str, opts: &FormatOptions, out: &mut dyn Write) -> std::io::Result<()> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let event: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            let preview: String = trimmed.chars().take(40).collect();
            return writeln!(out, "[malformed line: {preview}]");
        }
    };

    let event_type = match event.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => {
            let preview: String = trimmed.chars().take(40).collect();
            return writeln!(out, "[malformed line: {preview}]");
        }
    };

    match event_type {
        "assistant" => render_assistant(&event, opts, out),
        "user" => render_user(&event, opts, out),
        "system" | "rate_limit_event" => Ok(()), // suppressed; caller counts for footer
        other => writeln!(out, "[unknown event: {other}]"),
    }
}

/// Render an `"assistant"` event.
fn render_assistant(
    event: &Value,
    opts: &FormatOptions,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let content = match event
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        Some(arr) => arr,
        None => return Ok(()),
    };

    for block in content {
        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match block_type {
            "text" => {
                let text = block.get("text").and_then(|v| v.as_str()).unwrap_or("");
                write!(out, "{text}")?;
                writeln!(out, "\n---")?;
            }
            "tool_use" => {
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("UnknownTool");
                let input = block.get("input");
                let args_str = render_tool_args(input);
                writeln!(out, "-> {name}({args_str})")?;
            }
            "thinking" if opts.show_thinking => {
                let thinking = block.get("thinking").and_then(|v| v.as_str()).unwrap_or("");
                let first_line = thinking.lines().next().unwrap_or("");
                writeln!(out, "[thinking] {first_line}")?;
            }
            "thinking" => {} // suppressed when show_thinking is false
            _ => {}          // Unknown block type within assistant — skip silently
        }
    }
    Ok(())
}

/// Render a `"user"` event.
///
/// The user event may have:
/// - `message.content[]` with `tool_result` blocks
/// - A top-level `tool_use_result` field (dedup: skip if content matches last rendered tool_result)
fn render_user(event: &Value, opts: &FormatOptions, out: &mut dyn Write) -> std::io::Result<()> {
    // Collect tool_result content strings we render so we can dedup tool_use_result.
    let mut rendered_tool_result_content: Option<String> = None;

    let content = event
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array());

    if let Some(blocks) = content {
        for block in blocks {
            let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if block_type == "tool_result" {
                let content_str = block.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let is_error = block
                    .get("is_error")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                render_tool_result(content_str, is_error, out)?;
                rendered_tool_result_content = Some(content_str.to_string());
            }
        }
    }

    // Dedup: skip top-level tool_use_result if its content matches the last tool_result rendered.
    if let Some(tur) = event.get("tool_use_result") {
        let tur_content = tur.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let already_rendered = rendered_tool_result_content
            .as_deref()
            .map(|c| c == tur_content)
            .unwrap_or(false);

        if !already_rendered {
            let is_error = tur
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            render_tool_result(tur_content, is_error, out)?;
        }
    }

    // Suppress unused warning for opts — it's part of the public API
    let _ = opts;

    Ok(())
}

/// Render a single tool result line.
fn render_tool_result(content: &str, is_error: bool, out: &mut dyn Write) -> std::io::Result<()> {
    if is_error {
        let first_line = content.lines().next().unwrap_or("");
        writeln!(out, "<- error: {first_line}")
    } else if content.starts_with("PreToolUse:") {
        writeln!(out, "[hook block]")
    } else if content.contains("LOOM_HOOK_WARN:") {
        writeln!(out, "[hook warn]")
    } else {
        let bytes = content.len();
        writeln!(out, "<- ok ({bytes} bytes)")
    }
}

/// Render tool input arguments as `key=value, ...` pairs.
///
/// String values longer than 80 chars are truncated to 77 chars + `"..."`.
/// Non-string JSON values are rendered with `serde_json::to_string`.
fn render_tool_args(input: Option<&Value>) -> String {
    let obj = match input.and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return String::new(),
    };

    if obj.is_empty() {
        return String::new();
    }

    obj.iter()
        .map(|(k, v)| {
            let val_str = if let Some(s) = v.as_str() {
                if s.len() > 80 {
                    let truncated: String = s.chars().take(77).collect();
                    format!("{truncated}...")
                } else {
                    s.to_string()
                }
            } else {
                serde_json::to_string(v).unwrap_or_default()
            };
            format!("{k}={val_str}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Format all lines from `reader`, writing rendered output to `writer`.
///
/// Reads until EOF. When `opts.verbose` is true and there were suppressed
/// `system` or `rate_limit_event` events, appends a footer:
/// `--- [N system event(s), M rate-limit event(s) suppressed] ---\n`
pub fn format_stream<R: BufRead, W: Write>(
    reader: R,
    opts: &FormatOptions,
    writer: &mut W,
) -> std::io::Result<()> {
    let mut system_count: usize = 0;
    let mut rate_limit_count: usize = 0;

    for line_result in reader.lines() {
        let line = line_result?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Count suppressed event types before delegating to format_line.
        if opts.verbose {
            if let Ok(event) = serde_json::from_str::<Value>(trimmed) {
                match event.get("type").and_then(|v| v.as_str()) {
                    Some("system") => system_count += 1,
                    Some("rate_limit_event") => rate_limit_count += 1,
                    _ => {}
                }
            }
        }

        format_line(trimmed, opts, writer)?;
    }

    if opts.verbose && (system_count > 0 || rate_limit_count > 0) {
        writeln!(
            writer,
            "--- [{system_count} system event(s), {rate_limit_count} rate-limit event(s) suppressed] ---"
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn opts(show_thinking: bool, verbose: bool) -> FormatOptions {
        FormatOptions {
            show_thinking,
            verbose,
        }
    }

    fn run(line: &str, show_thinking: bool) -> String {
        let mut buf = Vec::new();
        format_line(line, &opts(show_thinking, false), &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    fn stream(input: &str, show_thinking: bool, verbose: bool) -> String {
        let mut buf = Vec::new();
        let cursor = Cursor::new(input.as_bytes());
        format_stream(cursor, &opts(show_thinking, verbose), &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn malformed_json_prints_diagnostic() {
        let out = run("not json at all", false);
        assert!(out.contains("[malformed line:"), "got: {out}");
        assert!(out.contains("not json at all"), "got: {out}");
    }

    #[test]
    fn malformed_json_truncated_at_40_chars() {
        let long = format!("x{}", "a".repeat(50));
        let out = run(&long, false);
        assert!(out.contains("[malformed line:"), "got: {out}");
        // 40 chars of preview
        let preview: String = long.chars().take(40).collect();
        assert!(out.contains(&preview), "got: {out}");
    }

    #[test]
    fn assistant_text_block_renders_with_separator() {
        let line =
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello world"}]}}"#;
        let out = run(line, false);
        assert!(out.contains("Hello world"), "got: {out}");
        assert!(out.contains("\n---"), "got: {out}");
    }

    #[test]
    fn assistant_tool_use_renders_name_and_args() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"ls -la","description":"list files"}}]}}"#;
        let out = run(line, false);
        assert!(out.contains("-> Bash("), "got: {out}");
        assert!(out.contains("command=ls -la"), "got: {out}");
        assert!(out.contains("description=list files"), "got: {out}");
    }

    #[test]
    fn tool_use_truncates_long_string_args() {
        let long_val = "a".repeat(90);
        let line = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","name":"Write","input":{{"content":"{long_val}"}}}}]}}}}"#
        );
        let out = run(&line, false);
        // Should truncate to 77 chars + "..."
        assert!(out.contains("..."), "expected truncation, got: {out}");
        let truncated: String = long_val.chars().take(77).collect();
        assert!(out.contains(&truncated), "got: {out}");
    }

    #[test]
    fn tool_use_empty_input_renders_empty_parens() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"MyTool","input":{}}]}}"#;
        let out = run(line, false);
        assert!(out.contains("-> MyTool()"), "got: {out}");
    }

    #[test]
    fn thinking_block_suppressed_by_default() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"internal thought\nsecond line"}]}}"#;
        let out = run(line, false);
        assert!(out.is_empty(), "expected empty, got: {out}");
    }

    #[test]
    fn thinking_block_rendered_with_flag() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"internal thought\nsecond line"}]}}"#;
        let out = run(line, true);
        assert!(out.contains("[thinking]"), "got: {out}");
        assert!(out.contains("internal thought"), "got: {out}");
        // Only the first line
        assert!(!out.contains("second line"), "got: {out}");
    }

    #[test]
    fn user_tool_result_ok_renders_bytes() {
        let content = "some result content";
        let line = format!(
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","content":"{content}","is_error":false}}]}}}}"#
        );
        let out = run(&line, false);
        let expected_bytes = content.len();
        assert!(
            out.contains(&format!("<- ok ({expected_bytes} bytes)")),
            "got: {out}"
        );
    }

    #[test]
    fn user_tool_result_error_renders_first_line() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"error occurred\nmore details","is_error":true}]}}"#;
        let out = run(line, false);
        assert!(out.contains("<- error: error occurred"), "got: {out}");
        assert!(!out.contains("more details"), "got: {out}");
    }

    #[test]
    fn user_tool_result_hook_block() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"PreToolUse: hook blocked this","is_error":false}]}}"#;
        let out = run(line, false);
        assert!(out.contains("[hook block]"), "got: {out}");
    }

    #[test]
    fn user_tool_result_hook_warn() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"some LOOM_HOOK_WARN: warning message","is_error":false}]}}"#;
        let out = run(line, false);
        assert!(out.contains("[hook warn]"), "got: {out}");
    }

    #[test]
    fn system_event_suppressed() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc"}"#;
        let out = run(line, false);
        assert!(out.is_empty(), "expected empty, got: {out}");
    }

    #[test]
    fn rate_limit_event_suppressed() {
        let line = r#"{"type":"rate_limit_event","delta":{"input_tokens":5}}"#;
        let out = run(line, false);
        assert!(out.is_empty(), "expected empty, got: {out}");
    }

    #[test]
    fn unknown_event_type_renders_diagnostic() {
        let line = r#"{"type":"foobar_unknown"}"#;
        let out = run(line, false);
        assert!(
            out.contains("[unknown event: foobar_unknown]"),
            "got: {out}"
        );
    }

    #[test]
    fn format_stream_collects_verbose_footer() {
        let input = concat!(
            r#"{"type":"system","subtype":"init"}"#,
            "\n",
            r#"{"type":"rate_limit_event"}"#,
            "\n",
            r#"{"type":"system"}"#,
            "\n",
        );
        let out = stream(input, false, true);
        assert!(out.contains("2 system event(s)"), "got: {out}");
        assert!(out.contains("1 rate-limit event(s)"), "got: {out}");
        assert!(out.contains("suppressed"), "got: {out}");
    }

    #[test]
    fn format_stream_no_footer_when_no_suppressed_events() {
        let input = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#;
        let out = stream(input, false, true);
        assert!(!out.contains("suppressed"), "got: {out}");
    }

    #[test]
    fn format_stream_no_footer_when_not_verbose() {
        let input = concat!(
            r#"{"type":"system"}"#,
            "\n",
            r#"{"type":"rate_limit_event"}"#,
            "\n",
        );
        let out = stream(input, false, false);
        assert!(!out.contains("suppressed"), "got: {out}");
    }

    #[test]
    fn tool_use_result_dedup_when_matches_tool_result() {
        // tool_use_result content matches the tool_result content => skip
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"same content","is_error":false}]},"tool_use_result":{"content":"same content","is_error":false}}"#;
        let out = run(line, false);
        // Should appear exactly once: "19 bytes" for "same content"
        let count = out.matches("<- ok").count();
        assert_eq!(count, 1, "expected 1 occurrence, got: {out}");
    }

    #[test]
    fn tool_use_result_rendered_when_no_tool_result_block() {
        let line = r#"{"type":"user","message":{"content":[]},"tool_use_result":{"content":"standalone result","is_error":false}}"#;
        let out = run(line, false);
        assert!(out.contains("<- ok"), "got: {out}");
    }

    #[test]
    fn empty_line_is_skipped() {
        let mut buf = Vec::new();
        format_line("", &opts(false, false), &mut buf).unwrap();
        format_line("   ", &opts(false, false), &mut buf).unwrap();
        assert!(buf.is_empty());
    }
}
