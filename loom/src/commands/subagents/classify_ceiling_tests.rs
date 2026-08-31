//! Coverage for subagent peak-token ceiling handling.

use std::fs;

use super::*;

fn transcript_with_input_tokens(directory: &std::path::Path) -> std::path::PathBuf {
    let path = directory.join("agent-x.jsonl");
    let row = serde_json::json!({
        "type": "assistant",
        "requestId": "req-1",
        "timestamp": "2020-01-01T00:00:00.000Z",
        "message": {
            "role": "assistant",
            "model": "claude-sonnet-5",
            "content": [{"type": "text", "text": "done"}],
            "usage": {"input_tokens": 40_000},
        },
    });
    fs::write(&path, format!("{row}\n")).unwrap();
    path
}

#[test]
fn peak_marker_uses_the_configured_subagent_ceiling() {
    let transcript_dir = tempfile::tempdir().unwrap();
    let path = transcript_with_input_tokens(transcript_dir.path());
    let work_dir = tempfile::tempdir().unwrap();
    crate::fs::work_dir::write_context_config(
        work_dir.path(),
        &crate::fs::work_dir::ContextConfig {
            ceiling_tokens: 150_000,
            subagent_ceiling_tokens: 30_000,
            model_window_tokens: None,
        },
    )
    .unwrap();

    let summary = analyze(
        &path,
        "x".to_owned(),
        DEFAULT_DONE_DEBOUNCE_SECS,
        Some(work_dir.path()),
    )
    .unwrap();

    assert!(summary.peak_tokens_over_ceiling);
}

#[test]
fn zero_subagent_ceiling_disables_the_peak_marker_like_the_hook() {
    let transcript_dir = tempfile::tempdir().unwrap();
    let path = transcript_with_input_tokens(transcript_dir.path());

    let summary =
        analyze_at_ceiling(&path, "x".to_owned(), DEFAULT_DONE_DEBOUNCE_SECS, None, 0).unwrap();

    assert!(!summary.peak_tokens_over_ceiling);
}
