//! Tests for `classify.rs`'s liveness table, including the `done` debounce,
//! the two classifier shapes the empirical census surfaced (`thinking`-only
//! and a `user`-role text entry), `tool-wait`'s immunity to idle time, and
//! the `.work/subagents/.../<agentId>.json` authoritative fast path. Split
//! out to keep `classify.rs` itself under the 400-line ceiling (CLAUDE.md
//! Rule 17).

use super::*;

fn write_transcript(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

/// Historical, not "now": every test below except the debounce-specific
/// ones exercises pure structural classification, so its idle time must
/// unconditionally clear `DEFAULT_DONE_DEBOUNCE_SECS` regardless of when
/// the suite actually runs.
const AGED_TIMESTAMP: &str = "2020-01-01T00:00:00.000Z";

fn line(entry_type: &str, content: Value) -> String {
    serde_json::json!({
        "type": entry_type,
        "timestamp": AGED_TIMESTAMP,
        "message": {"role": entry_type, "content": content},
    })
    .to_string()
}

#[test]
fn done_state_has_text_block_no_tool_use() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n{}\n",
        line("user", serde_json::json!("do the thing")),
        line(
            "assistant",
            serde_json::json!([{"type": "text", "text": "all done, here's the report"}])
        ),
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(
        summary.final_report.as_deref(),
        Some("all done, here's the report")
    );
    assert_eq!(summary.turns, 1);
}

#[test]
fn tool_wait_state_has_tool_use_block() {
    let temp = tempfile::tempdir().unwrap();
    let content = line(
        "assistant",
        serde_json::json!([
            {"type": "text", "text": "let me check"},
            {"type": "tool_use", "name": "Read", "input": {}}
        ]),
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &format!("{content}\n"));

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::ToolWait);
    assert_eq!(summary.last_tool.as_deref(), Some("Read"));
    assert!(summary.final_report.is_none());
}

#[test]
fn generating_state_when_last_entry_is_user_tool_result() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n{}\n",
        line(
            "assistant",
            serde_json::json!([{"type": "tool_use", "name": "Bash", "input": {}}])
        ),
        line(
            "user",
            serde_json::json!([{"type": "tool_result", "content": "ok"}])
        ),
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Generating);
    // last_tool still reflects the most recent tool_use, even though the
    // last entry itself is the user-role tool result.
    assert_eq!(summary.last_tool.as_deref(), Some("Bash"));
}

/// `user[text]` (sent a message, hasn't replied yet) is a distinct census
/// shape from `user[tool_result]` but maps to the same `generating` state.
#[test]
fn generating_state_when_last_entry_is_user_text() {
    let temp = tempfile::tempdir().unwrap();
    let content = line("user", serde_json::json!("are you still there?"));
    let path = write_transcript(temp.path(), "agent-x.jsonl", &format!("{content}\n"));

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Generating);
}

/// `assistant[thinking]` with no text and no tool_use: still mid-turn.
#[test]
fn thinking_only_entry_is_generating() {
    let temp = tempfile::tempdir().unwrap();
    let content = line(
        "assistant",
        serde_json::json!([{"type": "thinking", "thinking": "weighing the options"}]),
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &format!("{content}\n"));

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Generating);
    assert!(summary.final_report.is_none());
}

#[test]
fn unknown_state_for_unrecognized_shape() {
    let temp = tempfile::tempdir().unwrap();
    let content = line("summary", serde_json::json!("compacted"));
    let path = write_transcript(temp.path(), "agent-x.jsonl", &format!("{content}\n"));

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Unknown);
}

#[test]
fn torn_last_line_degrades_to_previous_good_entry() {
    let temp = tempfile::tempdir().unwrap();
    let good = line(
        "assistant",
        serde_json::json!([{"type": "text", "text": "finished before the tear"}]),
    );
    let content = format!("{good}\n{{\"type\": \"assistant\", \"mess");
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(
        summary.final_report.as_deref(),
        Some("finished before the tear")
    );
}

#[test]
fn empty_file_is_unknown_not_an_error() {
    let temp = tempfile::tempdir().unwrap();
    let path = write_transcript(temp.path(), "agent-x.jsonl", "");

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Unknown);
    assert_eq!(summary.turns, 0);
    assert!(summary.last_tool.is_none());
    assert!(summary.final_report.is_none());
    assert!(summary.request_count.is_none());
}

#[test]
fn missing_file_is_an_error() {
    let missing = Path::new("/nonexistent/agent-x.jsonl");
    assert!(analyze(missing, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).is_err());
}

/// (a) A structurally-done last entry with a FRESH timestamp must not be
/// reported as `done` -- it may still be mid-turn, the false positive this
/// whole debounce exists to prevent.
#[test]
fn fresh_text_only_entry_is_generating_not_done() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": Utc::now().to_rfc3339(),
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "still narrating, more to come"}],
            },
        })
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Generating);
    assert!(summary.final_report.is_none());
}

/// (b) The same shape, aged past the debounce, is trusted as `done`.
#[test]
fn text_only_entry_aged_past_debounce_is_done() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": AGED_TIMESTAMP,
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "genuinely finished"}],
            },
        })
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(summary.final_report.as_deref(), Some("genuinely finished"));
}

/// `tool-wait` must never debounce or time out, at any idle time -- a real
/// tool call in this codebase has been measured running 603s, and the
/// overall max is 1,425s (23.8 minutes). 30 minutes idle must still read
/// as `tool-wait`, never `done` and never `unknown`.
#[test]
fn tool_wait_never_debounces_even_after_30_minutes_idle() {
    let temp = tempfile::tempdir().unwrap();
    let old_timestamp = (Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": old_timestamp,
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "name": "Bash", "input": {}}],
            },
        })
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::ToolWait);
    assert!(summary.idle_secs >= 1800);
    assert!(summary.final_report.is_none());
}

/// An authoritative `.work/subagents/<stage>/<agentId>.json` record forces
/// `done` immediately, bypassing the debounce entirely -- even for a
/// transcript whose last entry has a fresh timestamp.
#[test]
fn authoritative_termination_record_forces_done_with_no_debounce() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": Utc::now().to_rfc3339(),
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "the hook already fired for this one"}],
            },
        })
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let work_dir = tempfile::tempdir().unwrap();
    let stage_dir = work_dir.path().join("subagents").join("some-stage");
    fs::create_dir_all(&stage_dir).unwrap();
    fs::write(stage_dir.join("x.json"), "{}").unwrap();

    let summary = analyze(
        &path,
        "x".to_string(),
        DEFAULT_DONE_DEBOUNCE_SECS,
        Some(work_dir.path()),
    )
    .unwrap();
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(
        summary.final_report.as_deref(),
        Some("the hook already fired for this one")
    );
}

/// No record for this agent (the `subagents/` tree exists, but nothing
/// names this `agentId`) falls back to the ordinary debounce rule.
#[test]
fn missing_termination_record_falls_back_to_transcript_rule() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": Utc::now().to_rfc3339(),
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "no hook record for this one"}],
            },
        })
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &content);

    let work_dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(work_dir.path().join("subagents").join("some-stage")).unwrap();
    // No `x.json` written in that stage dir.

    let summary = analyze(
        &path,
        "x".to_string(),
        DEFAULT_DONE_DEBOUNCE_SECS,
        Some(work_dir.path()),
    )
    .unwrap();
    assert_eq!(summary.state, SubagentState::Generating);
}

/// A missing `.work/` root (no `subagents/` dir at all) degrades silently
/// rather than erroring, per the "never depend on it existing" contract.
#[test]
fn missing_work_dir_degrades_silently() {
    let temp = tempfile::tempdir().unwrap();
    let content = line(
        "assistant",
        serde_json::json!([{"type": "text", "text": "no .work here"}]),
    );
    let path = write_transcript(temp.path(), "agent-x.jsonl", &format!("{content}\n"));

    let summary = analyze(
        &path,
        "x".to_string(),
        DEFAULT_DONE_DEBOUNCE_SECS,
        Some(Path::new("/nonexistent/work/dir")),
    )
    .unwrap();
    assert_eq!(summary.state, SubagentState::Done);
    assert!(summary.agent_type.is_none());
}

/// One `assistant` transcript row aged past the debounce, with a distinct
/// `requestId` and token usage, for `analyze_carries_agent_type_model_requests_and_peak_tokens`.
fn assistant_row(request_id: &str, content: Value, input_tokens: u64) -> Value {
    serde_json::json!({
        "type": "assistant",
        "requestId": request_id,
        "timestamp": AGED_TIMESTAMP,
        "message": {
            "role": "assistant",
            "model": "claude-sonnet-5",
            "content": content,
            "usage": {"input_tokens": input_tokens},
        },
    })
}

/// End-to-end coverage for the four fields `classify::analyze` threads
/// through `summary::with_last` (`agent_type`, `model`, `request_count`,
/// `peak_resident_tokens`): leaf-only tests on `ledger.rs`, `metrics_tests.rs`
/// and `table.rs` cannot catch a swapped or dropped argument at the
/// `summary::with_last` call site, since none of them go through `analyze`.
#[test]
fn analyze_carries_agent_type_model_requests_and_peak_tokens() {
    let temp = tempfile::tempdir().unwrap();
    let tool_use = serde_json::json!([{"type": "tool_use", "name": "Read", "input": {}}]);
    let text = serde_json::json!([{"type": "text", "text": "done"}]);
    let content = format!(
        "{}\n{}\n",
        assistant_row("req-1", tool_use, 1_000),
        assistant_row("req-2", text, 2_000),
    );
    let path = write_transcript(temp.path(), "agent-y.jsonl", &content);

    let work_dir = tempfile::tempdir().unwrap();
    let stage_dir = work_dir.path().join("subagents").join("stage-a");
    fs::create_dir_all(&stage_dir).unwrap();
    fs::write(
        stage_dir.join("starts.jsonl"),
        "{\"agent_id\":\"y\",\"agent_type\":\"review\"}\n",
    )
    .unwrap();

    let summary = analyze(
        &path,
        "y".to_string(),
        DEFAULT_DONE_DEBOUNCE_SECS,
        Some(work_dir.path()),
    )
    .unwrap();

    assert_eq!(summary.agent_type.as_deref(), Some("review"));
    assert_eq!(summary.model.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(summary.request_count, Some(2));
    assert_eq!(summary.peak_resident_tokens, Some(2000));
}
