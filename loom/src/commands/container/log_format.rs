//! Stream-json (JSONL) log formatter for Claude Code container sessions.
//!
//! Per-tool human-readable rendering. Each tool_use → tool_result pair is shown as:
//!
//! ```text
//!   <Tool>  <head summary>
//!           <body line>
//!           <body line>
//! ```
//!
//! Head is indented 2 spaces; body lines are indented 8 spaces. Errors prefix
//! the first body line with `✗` (red when color is on). Hook-block / hook-warn
//! tool_results render as `✗ blocked by hook` / `⚠ hook warn`. A trailing blank
//! line follows each tool block for readability.

use clap::ValueEnum;
use serde_json::Value;
use std::collections::HashMap;
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
#[derive(Default, Clone)]
pub struct FormatOptions {
    /// Emit `[thinking]` prefixed lines for assistant thinking blocks.
    pub show_thinking: bool,
    /// Append a suppressed-events footer and count rate-limit / system events.
    pub verbose: bool,
    /// Emit ANSI color escapes. Callers should resolve TTY detection / NO_COLOR
    /// before constructing FormatOptions.
    pub color: bool,
}

/// Per-stream state for tool_use → tool_result pairing across lines.
#[derive(Default)]
pub struct FormatState {
    pending: HashMap<String, PendingTool>,
}

struct PendingTool {
    name: String,
    args: Value,
}

const HEAD_INDENT: &str = "  ";
const BODY_INDENT: &str = "        ";

// ── ANSI helpers ────────────────────────────────────────────────────────

fn ansi(color: bool, code: &str, text: &str) -> String {
    if color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn red(color: bool, text: &str) -> String {
    ansi(color, "31", text)
}
fn yellow(color: bool, text: &str) -> String {
    ansi(color, "33", text)
}
fn dim(color: bool, text: &str) -> String {
    ansi(color, "2", text)
}

// ── String helpers ──────────────────────────────────────────────────────

/// Replace control characters with a space so an adversarial log producer
/// cannot inject terminal escape sequences via head / preview strings.
fn sanitize_inline(s: &str, max_chars: usize) -> String {
    s.chars()
        .take(max_chars)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn truncate(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

fn count_lines(s: &str) -> usize {
    if s.is_empty() {
        0
    } else {
        s.lines().count()
    }
}

fn plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("{n} {singular}")
    } else {
        format!("{n} {plural}")
    }
}

/// Shorten a container path for display:
///   /repo/.worktrees/<id>/loom/src/x.rs  →  loom/src/x.rs
///   /repo/Cargo.toml                     →  Cargo.toml
///   /home/loom/.claude/x                 →  ~/.claude/x
///   /home/user/foo/bar                   →  ~/foo/bar
/// If still over 60 chars and > 4 path components, collapse the middle with `…`.
fn shorten_path(path: &str) -> String {
    let trimmed = if let Some(rest) = path.strip_prefix("/repo/.worktrees/") {
        match rest.split_once('/') {
            Some((_, after)) => after.to_string(),
            None => rest.to_string(),
        }
    } else if let Some(rest) = path.strip_prefix("/repo/") {
        rest.to_string()
    } else if let Some(rest) = path.strip_prefix("/home/loom/") {
        format!("~/{rest}")
    } else if let Some(rest) = path.strip_prefix("/home/") {
        match rest.split_once('/') {
            Some((_, after)) => format!("~/{after}"),
            None => path.to_string(),
        }
    } else {
        path.to_string()
    };

    const MAX: usize = 60;
    if trimmed.len() <= MAX {
        return trimmed;
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() <= 4 {
        return trimmed;
    }
    let n = parts.len();
    format!(
        "{}/{}/…/{}/{}",
        parts[0],
        parts[1],
        parts[n - 2],
        parts[n - 1]
    )
}

// ── Entry point ─────────────────────────────────────────────────────────

/// Format a single stream-json line, writing rendered output to `out`.
///
/// `state` is updated when an `assistant` event contains `tool_use` blocks so
/// that the subsequent `user` event's `tool_result` can render with knowledge
/// of which tool produced it.
pub fn format_line(
    line: &str,
    opts: &FormatOptions,
    state: &mut FormatState,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let event: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            let preview = sanitize_inline(trimmed, 40);
            return writeln!(out, "[malformed line: {preview}]");
        }
    };

    let event_type = match event.get("type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => {
            let preview = sanitize_inline(trimmed, 40);
            return writeln!(out, "[malformed line: {preview}]");
        }
    };

    match event_type {
        "assistant" => render_assistant(&event, opts, state, out),
        "user" => render_user(&event, opts, state, out),
        "system" | "rate_limit_event" => Ok(()), // counted in format_stream footer
        other => writeln!(out, "[unknown event: {}]", sanitize_inline(other, 40)),
    }
}

// ── Assistant rendering ─────────────────────────────────────────────────

fn render_assistant(
    event: &Value,
    opts: &FormatOptions,
    state: &mut FormatState,
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
                writeln!(out, "{text}")?;
                writeln!(out)?;
            }
            "tool_use" => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("UnknownTool")
                    .to_string();
                let args = block.get("input").cloned().unwrap_or(Value::Null);

                let summary = tool_head_summary(&name, &args);
                writeln!(out, "{HEAD_INDENT}{name}  {summary}")?;

                if !id.is_empty() {
                    state.pending.insert(id, PendingTool { name, args });
                }
            }
            "thinking" if opts.show_thinking => {
                let thinking = block.get("thinking").and_then(|v| v.as_str()).unwrap_or("");
                let first_line = thinking.lines().next().unwrap_or("");
                writeln!(
                    out,
                    "{} {}",
                    dim(opts.color, "[thinking]"),
                    dim(opts.color, first_line)
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

// ── User (tool result) rendering ────────────────────────────────────────

struct ResultBlock<'a> {
    id: Option<&'a str>,
    content: &'a str,
    is_error: bool,
}

fn render_user(
    event: &Value,
    opts: &FormatOptions,
    state: &mut FormatState,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let mut blocks: Vec<ResultBlock> = Vec::new();
    if let Some(arr) = event
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        for blk in arr {
            if blk.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                let id = blk.get("tool_use_id").and_then(|v| v.as_str());
                let content = blk.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let is_error = blk
                    .get("is_error")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                blocks.push(ResultBlock {
                    id,
                    content,
                    is_error,
                });
            }
        }
    }

    let tur = event.get("tool_use_result");
    let tur_content = tur
        .and_then(|t| t.get("content"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tur_is_error = tur
        .and_then(|t| t.get("is_error"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let mut rendered = false;

    for block in &blocks {
        // If inline tool_result is empty but tool_use_result top-level carries
        // real content, use the latter — this fuses the otherwise-duplicate
        // "ok (0 bytes)" + "ok (N bytes)" pair into one outcome line.
        let (content, is_error) = if block.content.is_empty() && !tur_content.is_empty() {
            (tur_content, block.is_error || tur_is_error)
        } else {
            (block.content, block.is_error)
        };

        let pending = block.id.and_then(|id| state.pending.remove(id));
        emit_tool_outcome(pending.as_ref(), content, is_error, opts, out)?;
        rendered = true;
    }

    if !rendered && !tur_content.is_empty() {
        emit_tool_outcome(None, tur_content, tur_is_error, opts, out)?;
    }

    Ok(())
}

fn emit_tool_outcome(
    pending: Option<&PendingTool>,
    content: &str,
    is_error: bool,
    opts: &FormatOptions,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let body = compute_body(pending, content, is_error);
    for line in &body {
        let styled = if line.starts_with('✗') {
            red(opts.color, line)
        } else if line.starts_with('⚠') {
            yellow(opts.color, line)
        } else {
            line.clone()
        };
        writeln!(out, "{BODY_INDENT}{styled}")?;
    }
    writeln!(out)?;
    Ok(())
}

fn compute_body(pending: Option<&PendingTool>, content: &str, is_error: bool) -> Vec<String> {
    // Hook special-form results (regardless of which tool produced them)
    if content.starts_with("PreToolUse:") {
        let msg = content.trim_start_matches("PreToolUse:").trim();
        let first = msg.lines().next().unwrap_or("");
        return vec![format!(
            "✗ blocked by hook: {}",
            truncate(&sanitize_inline(first, 200), 120)
        )];
    }
    if content.contains("LOOM_HOOK_WARN:") {
        let warn = content
            .split_once("LOOM_HOOK_WARN:")
            .map(|(_, after)| after)
            .unwrap_or("")
            .trim();
        let first = warn.lines().next().unwrap_or("");
        return vec![format!(
            "⚠ hook warn: {}",
            truncate(&sanitize_inline(first, 200), 120)
        )];
    }

    if is_error {
        let mut lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            lines.push("error");
        }
        let mut out = Vec::with_capacity(lines.len().min(10));
        out.push(format!(
            "✗ {}",
            truncate(&sanitize_inline(lines[0], 200), 120)
        ));
        for extra in lines.iter().skip(1).take(9) {
            out.push(truncate(&sanitize_inline(extra, 200), 120));
        }
        return out;
    }

    match pending.map(|p| p.name.as_str()) {
        Some("Read") => body_read(content),
        Some("Edit") | Some("MultiEdit") => body_edit(&pending.unwrap().args),
        Some("Write") => body_write(&pending.unwrap().args),
        Some("Bash") => body_bash(content),
        Some("Grep") => body_grep(content),
        Some("Glob") => body_glob(content),
        Some("TodoWrite") => body_todo(),
        Some("Task") => body_task(content),
        Some("WebFetch") | Some("WebSearch") => body_web(content),
        _ => body_generic(content),
    }
}

// ── Per-tool head summaries ─────────────────────────────────────────────

fn tool_head_summary(name: &str, args: &Value) -> String {
    match name {
        "Read" => head_read(args),
        "Edit" | "MultiEdit" => head_edit(args),
        "Write" => head_write(args),
        "Bash" => head_bash(args),
        "Grep" => head_grep(args),
        "Glob" => head_glob(args),
        "TodoWrite" => head_todo(args),
        "Task" => head_task(args),
        "WebFetch" => head_webfetch(args),
        "WebSearch" => head_websearch(args),
        _ => head_generic(args),
    }
}

fn head_read(args: &Value) -> String {
    let path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let offset = args.get("offset").and_then(|v| v.as_u64());
    let limit = args.get("limit").and_then(|v| v.as_u64());
    let short = shorten_path(path);
    match (offset, limit) {
        (Some(o), Some(l)) => format!("{short}:{o}-{}", o + l),
        (Some(o), None) => format!("{short}:{o}+"),
        (None, Some(l)) => format!("{short} (first {l} lines)"),
        (None, None) => short,
    }
}

fn body_read(content: &str) -> Vec<String> {
    if content.is_empty() {
        return vec!["(empty)".to_string()];
    }
    let lines = count_lines(content);
    vec![plural(lines, "line", "lines")]
}

fn head_edit(args: &Value) -> String {
    let path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let replace_all = args
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let short = shorten_path(path);
    if replace_all {
        format!("{short} (replace_all)")
    } else {
        short
    }
}

fn body_edit(args: &Value) -> Vec<String> {
    let old_s = args
        .get("old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_s = args
        .get("new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let replace_all = args
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let old_lines: Vec<&str> = old_s.lines().collect();
    let new_lines: Vec<&str> = new_s.lines().collect();

    let take = 3;
    let mut body = Vec::new();
    for line in old_lines.iter().take(take) {
        body.push(format!("- {}", truncate(&sanitize_inline(line, 200), 100)));
    }
    if old_lines.len() > take {
        body.push(format!("- … ({} more lines)", old_lines.len() - take));
    }
    for line in new_lines.iter().take(take) {
        body.push(format!("+ {}", truncate(&sanitize_inline(line, 200), 100)));
    }
    if new_lines.len() > take {
        body.push(format!("+ … ({} more lines)", new_lines.len() - take));
    }
    body.push(
        if replace_all {
            "all occurrences replaced"
        } else {
            "1 replacement"
        }
        .to_string(),
    );
    body
}

fn head_write(args: &Value) -> String {
    let path = args
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    shorten_path(path)
}

fn body_write(args: &Value) -> Vec<String> {
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let bytes = content.len();
    let lines = count_lines(content);
    vec![format!("wrote {bytes} bytes ({lines} lines)")]
}

fn head_bash(args: &Value) -> String {
    let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("?");
    truncate(&sanitize_inline(cmd, 240), 120)
}

fn body_bash(content: &str) -> Vec<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return vec![];
    }
    if lines.len() <= 3 {
        return lines
            .iter()
            .map(|l| truncate(&sanitize_inline(l, 200), 120))
            .collect();
    }
    vec![format!(
        "{} of output",
        plural(lines.len(), "line", "lines")
    )]
}

fn head_grep(args: &Value) -> String {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
    let path = args.get("path").and_then(|v| v.as_str());
    let pat_disp = truncate(&sanitize_inline(pattern, 100), 60);
    match path {
        Some(p) => format!("\"{pat_disp}\" in {}", shorten_path(p)),
        None => format!("\"{pat_disp}\""),
    }
}

fn body_grep(content: &str) -> Vec<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return vec!["0 matches".to_string()];
    }
    if let Some(first) = trimmed.lines().next() {
        if first.starts_with("Found ") {
            return vec![first.to_string()];
        }
    }
    let lines = count_lines(trimmed);
    vec![plural(lines, "match", "matches")]
}

fn head_glob(args: &Value) -> String {
    let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("?");
    let path = args.get("path").and_then(|v| v.as_str());
    let pat = truncate(&sanitize_inline(pattern, 100), 60);
    match path {
        Some(p) => format!("{pat} in {}", shorten_path(p)),
        None => pat,
    }
}

fn body_glob(content: &str) -> Vec<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return vec!["0 matches".to_string()];
    }
    let lines = count_lines(trimmed);
    vec![plural(lines, "match", "matches")]
}

fn head_todo(args: &Value) -> String {
    let todos = args.get("todos").and_then(|v| v.as_array());
    let Some(arr) = todos else {
        return String::new();
    };
    let mut done = 0;
    let mut in_progress = 0;
    let mut pending = 0;
    for t in arr {
        match t.get("status").and_then(|v| v.as_str()) {
            Some("completed") => done += 1,
            Some("in_progress") => in_progress += 1,
            _ => pending += 1,
        }
    }
    format!(
        "{} items ({done} done, {in_progress} in_progress, {pending} pending)",
        arr.len()
    )
}

fn body_todo() -> Vec<String> {
    vec![]
}

fn head_task(args: &Value) -> String {
    let typ = args
        .get("subagent_type")
        .and_then(|v| v.as_str())
        .unwrap_or("agent");
    let desc = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    format!("[{typ}] {}", truncate(&sanitize_inline(desc, 100), 80))
}

fn body_task(content: &str) -> Vec<String> {
    let bytes = content.len();
    if bytes == 0 {
        return vec!["(no output)".to_string()];
    }
    let lines = count_lines(content);
    vec![format!(
        "returned {bytes} bytes, {}",
        plural(lines, "line", "lines")
    )]
}

fn head_webfetch(args: &Value) -> String {
    let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("?");
    truncate(&sanitize_inline(url, 120), 100)
}

fn head_websearch(args: &Value) -> String {
    let q = args.get("query").and_then(|v| v.as_str()).unwrap_or("?");
    format!("\"{}\"", truncate(&sanitize_inline(q, 120), 80))
}

fn body_web(content: &str) -> Vec<String> {
    let bytes = content.len();
    if bytes == 0 {
        return vec!["(empty)".to_string()];
    }
    let first = content.lines().next().unwrap_or("");
    vec![
        format!("{bytes} bytes"),
        truncate(&sanitize_inline(first, 200), 120),
    ]
}

fn head_generic(args: &Value) -> String {
    let Some(obj) = args.as_object() else {
        return String::new();
    };
    if obj.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = obj
        .iter()
        .map(|(k, v)| {
            let s = if let Some(s) = v.as_str() {
                truncate(&sanitize_inline(s, 100), 60)
            } else {
                serde_json::to_string(v).unwrap_or_default()
            };
            format!("{k}={s}")
        })
        .collect();
    truncate(&parts.join(", "), 120)
}

fn body_generic(content: &str) -> Vec<String> {
    if content.is_empty() {
        return vec![];
    }
    let lines = count_lines(content);
    if lines > 1 {
        vec![format!("{lines} lines ({} bytes)", content.len())]
    } else {
        vec![truncate(&sanitize_inline(content, 200), 120)]
    }
}

// ── format_stream ───────────────────────────────────────────────────────

/// Format all lines from `reader`, writing rendered output to `writer`.
///
/// When `opts.verbose` is true and suppressed `system` / `rate_limit_event`
/// records were seen, appends a footer summarizing the counts.
pub fn format_stream<R: BufRead, W: Write>(
    reader: R,
    opts: &FormatOptions,
    writer: &mut W,
) -> std::io::Result<()> {
    let mut state = FormatState::default();
    let mut system_count: usize = 0;
    let mut rate_limit_count: usize = 0;

    for line_result in reader.lines() {
        let line = line_result?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if opts.verbose {
            if let Ok(event) = serde_json::from_str::<Value>(trimmed) {
                match event.get("type").and_then(|v| v.as_str()) {
                    Some("system") => system_count += 1,
                    Some("rate_limit_event") => rate_limit_count += 1,
                    _ => {}
                }
            }
        }

        format_line(trimmed, opts, &mut state, writer)?;
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
            color: false,
        }
    }

    fn run(line: &str, show_thinking: bool) -> String {
        let mut state = FormatState::default();
        let mut buf = Vec::new();
        format_line(line, &opts(show_thinking, false), &mut state, &mut buf).unwrap();
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
        let preview: String = long.chars().take(40).collect();
        assert!(out.contains(&preview), "got: {out}");
    }

    #[test]
    fn assistant_text_block_renders_with_blank_line() {
        let line =
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello world"}]}}"#;
        let out = run(line, false);
        assert!(out.contains("Hello world"), "got: {out}");
        assert!(!out.contains("---"), "no horizontal rule expected: {out}");
        assert!(
            out.ends_with("\n\n"),
            "expected trailing blank line: {out:?}"
        );
    }

    #[test]
    fn read_head_uses_short_path_and_line_range() {
        let input = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu1","name":"Read","input":{"file_path":"/repo/.worktrees/foo/loom/src/lib.rs","offset":10,"limit":50}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu1","content":"line1\nline2\nline3","is_error":false}]}}"#,
        );
        let out = stream(input, false, false);
        assert!(out.contains("  Read  loom/src/lib.rs:10-60"), "got: {out}");
        assert!(out.contains("        3 lines"), "got: {out}");
    }

    #[test]
    fn edit_renders_diff_and_replacement_count() {
        let input = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"e1","name":"Edit","input":{"file_path":"/repo/.worktrees/foo/src/x.rs","old_string":"let a = 1;","new_string":"let a = 2;"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"e1","content":"ok","is_error":false}]}}"#,
        );
        let out = stream(input, false, false);
        assert!(out.contains("  Edit  src/x.rs"), "got: {out}");
        assert!(out.contains("- let a = 1;"), "got: {out}");
        assert!(out.contains("+ let a = 2;"), "got: {out}");
        assert!(out.contains("1 replacement"), "got: {out}");
    }

    #[test]
    fn bash_success_short_output_shows_inline_lines() {
        let input = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b1","name":"Bash","input":{"command":"ls -la"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b1","content":"file1.txt\nfile2.txt","is_error":false}]}}"#,
        );
        let out = stream(input, false, false);
        assert!(out.contains("  Bash  ls -la"), "got: {out}");
        assert!(out.contains("file1.txt"), "got: {out}");
        assert!(out.contains("file2.txt"), "got: {out}");
    }

    #[test]
    fn bash_long_output_shows_line_count() {
        let body: Vec<String> = (1..=20).map(|i| format!("line{i}")).collect();
        let body_json = body.join("\\n");
        let user_line = format!(
            r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"b2","content":"{body_json}","is_error":false}}]}}}}"#
        );
        let input = format!(
            "{}\n{}",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b2","name":"Bash","input":{"command":"cargo test"}}]}}"#,
            user_line
        );
        let out = stream(&input, false, false);
        assert!(out.contains("  Bash  cargo test"), "got: {out}");
        assert!(out.contains("20 lines of output"), "got: {out}");
    }

    #[test]
    fn bash_failure_shows_x_marker_and_first_lines() {
        let input = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"b3","name":"Bash","input":{"command":"cargo build"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"b3","content":"error[E0412]: cannot find type `Foo`\n  --> src/lib.rs:10:5\n  not declared","is_error":true}]}}"#,
        );
        let out = stream(input, false, false);
        assert!(out.contains("  Bash  cargo build"), "got: {out}");
        assert!(out.contains("✗ error[E0412]"), "got: {out}");
        assert!(out.contains("--> src/lib.rs:10:5"), "got: {out}");
    }

    #[test]
    fn empty_inline_result_uses_top_level_tool_use_result() {
        // Reproduces the "<- ok (0 bytes)" duplicate bug from the old formatter:
        // message.content[].tool_result is empty, but top-level tool_use_result has the real content.
        let input = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"r1","name":"Read","input":{"file_path":"/repo/x.rs"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"r1","content":"","is_error":false}]},"tool_use_result":{"content":"line1\nline2\nline3"}}"#,
        );
        let out = stream(input, false, false);
        // Should render once with the real content (3 lines), not twice (0 + 3).
        let blank_lines = out.matches("\n\n").count();
        assert!(out.contains("3 lines"), "got: {out}");
        assert!(blank_lines <= 1, "expected single block, got: {out}");
    }

    #[test]
    fn todo_head_shows_status_summary() {
        let input = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"TodoWrite","input":{"todos":[{"status":"completed"},{"status":"completed"},{"status":"in_progress"},{"status":"pending"}]}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"ok","is_error":false}]}}"#,
        );
        let out = stream(input, false, false);
        assert!(
            out.contains("4 items (2 done, 1 in_progress, 1 pending)"),
            "got: {out}"
        );
    }

    #[test]
    fn grep_head_shows_pattern_and_path() {
        let input = concat!(
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"g1","name":"Grep","input":{"pattern":"TODO","path":"/repo/.worktrees/foo/src"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"g1","content":"a.rs\nb.rs\nc.rs","is_error":false}]}}"#,
        );
        let out = stream(input, false, false);
        assert!(out.contains("Grep  \"TODO\" in src"), "got: {out}");
        assert!(out.contains("3 matches"), "got: {out}");
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
        assert!(!out.contains("second line"), "got: {out}");
    }

    #[test]
    fn hook_block_rendered_with_x_marker() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu","content":"PreToolUse: git commit blocked by commit-filter.sh","is_error":false}]}}"#;
        let out = run(line, false);
        assert!(out.contains("✗ blocked by hook"), "got: {out}");
        assert!(out.contains("commit-filter.sh"), "got: {out}");
    }

    #[test]
    fn hook_warn_rendered_with_warn_marker() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tu","content":"LOOM_HOOK_WARN: suspicious pattern","is_error":false}]}}"#;
        let out = run(line, false);
        assert!(out.contains("⚠ hook warn"), "got: {out}");
        assert!(out.contains("suspicious pattern"), "got: {out}");
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
    fn verbose_footer_collects_suppressed_events() {
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
    }

    #[test]
    fn empty_line_is_skipped() {
        let mut state = FormatState::default();
        let mut buf = Vec::new();
        format_line("", &opts(false, false), &mut state, &mut buf).unwrap();
        format_line("   ", &opts(false, false), &mut state, &mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn malformed_preview_strips_control_chars() {
        let line = "\x1b[2J\x1b[Hnot json\x07\n";
        let out = run(line, false);
        assert!(out.starts_with("[malformed line:"), "got: {out}");
        assert!(!out.contains('\x1b'), "ESC must be sanitized: {out:?}");
        assert!(!out.contains('\x07'), "BEL must be sanitized: {out:?}");
    }

    #[test]
    fn shorten_path_strips_worktree_prefix() {
        assert_eq!(
            shorten_path("/repo/.worktrees/my-stage/loom/src/lib.rs"),
            "loom/src/lib.rs"
        );
        assert_eq!(shorten_path("/repo/Cargo.toml"), "Cargo.toml");
        assert_eq!(
            shorten_path("/home/loom/.claude/hooks/commit-filter.sh"),
            "~/.claude/hooks/commit-filter.sh"
        );
        assert_eq!(shorten_path("/home/alice/foo/bar.rs"), "~/foo/bar.rs");
    }

    #[test]
    fn shorten_path_collapses_long_middle() {
        let long = "/repo/aaa/bbb/ccc/ddd/eee/fff/ggg/hhh/iii/jjj/very_long_filename_here.rs";
        let out = shorten_path(long);
        assert!(out.starts_with("aaa/bbb/"), "got: {out}");
        assert!(
            out.ends_with("jjj/very_long_filename_here.rs"),
            "got: {out}"
        );
        assert!(out.contains("…"), "got: {out}");
    }
}
