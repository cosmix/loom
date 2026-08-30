//! Regression tests for two failure-recovery defects in `analyze`: a torn
//! multibyte UTF-8 tail must not fail the whole read (previously
//! `fs::read_to_string` errored on any invalid byte, dropping the agent from
//! `watch`'s settled check entirely), and an authoritative `Done` with no
//! text blocks in its last entry must not report an empty string as a
//! harvestable final report. Split out of `classify_tests.rs`, which is at
//! the file-size ceiling (CLAUDE.md Rule 17).

use super::*;

fn write_transcript(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    path
}

/// A transcript truncated mid-multibyte-character (a real possibility: the
/// file is being appended to while this reads it) must still yield the last
/// good entry rather than failing the whole read. Before the fix,
/// `fs::read_to_string` errored on the invalid byte and `analyze` returned
/// `Err`, which `render.rs`'s `gather` turns into a dropped agent -- and a
/// `watch` poll evaluated over the survivors could then report "every
/// subagent is done" while this one was silently missing.
#[test]
fn torn_multibyte_utf8_tail_does_not_fail_the_whole_read() {
    let temp = tempfile::tempdir().unwrap();
    let good = serde_json::json!({
        "type": "assistant",
        "timestamp": "2020-01-01T00:00:00.000Z",
        "message": {
            "role": "assistant",
            "content": [{"type": "text", "text": "finished before the torn byte"}],
        },
    })
    .to_string();
    let mut bytes = format!("{good}\n").into_bytes();
    // A lone continuation byte of a 3-byte UTF-8 sequence: invalid on its
    // own, appended with no closing bytes to simulate an in-progress write.
    bytes.extend_from_slice(&[0xE2, 0x82]);
    let path = temp.path().join("agent-x.jsonl");
    fs::write(&path, &bytes).unwrap();

    let summary = analyze(&path, "x".to_string(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
    assert_eq!(summary.state, SubagentState::Done);
    assert_eq!(
        summary.final_report.as_deref(),
        Some("finished before the torn byte")
    );
}

/// An authoritative termination record can force `Done` even when the last
/// flushed transcript entry is a `tool_use` block with no text -- the
/// transcript may simply be lagging behind the hook. `final_report` must be
/// `None` in that case, not `Some("")`: `harvest` gates its print branch on
/// `final_report.is_some()`, so an empty-string report used to print a bare
/// header with no body and still count as "harvested".
#[test]
fn authoritative_done_with_no_text_blocks_has_no_final_report() {
    let temp = tempfile::tempdir().unwrap();
    let content = format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant",
            "timestamp": Utc::now().to_rfc3339(),
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "name": "Bash", "input": {}}],
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
    assert!(
        summary.final_report.is_none(),
        "an authoritative Done with no text blocks must not report an empty string as harvestable"
    );
}
