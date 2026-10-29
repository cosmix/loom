//! Contracts: a transcript's cost is counted by the token tracker's accumulator, and an estimate
//! falls back from a summary's last cost to the per-byte median to the starting figure.

use super::estimate::{estimate, EstimateBasis};
use super::{transcript_tokens, CostLine, Level, TokenVector, Trigger};
use crate::tokens::accumulate::AgentState;
use crate::tokens::AgentKind;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/plan_session/transcripts/streamed.jsonl"
);

/// Two more lines of one streamed message: the same `message.id`, the output growing.
const STREAMED: [&str; 2] = [
    r#"{"type":"assistant","uuid":"b1","parentUuid":"a3","isSidechain":false,"timestamp":"2026-10-27T10:00:04.000Z","sessionId":"105e2c4a-7f1b-4c3d-9a8e-2b6d0f1e3a57","requestId":"req_011CXb","message":{"id":"msg_011CStream2","model":"claude-opus-5-5","role":"assistant","content":[{"type":"text","text":"Two runs failed."}],"stop_reason":null,"usage":{"input_tokens":4,"cache_creation_input_tokens":120,"cache_read_input_tokens":57000,"output_tokens":5}},"apiBlockIndex":0}"#,
    r#"{"type":"assistant","uuid":"b2","parentUuid":"b1","isSidechain":false,"timestamp":"2026-10-27T10:00:05.000Z","sessionId":"105e2c4a-7f1b-4c3d-9a8e-2b6d0f1e3a57","requestId":"req_011CXb","message":{"id":"msg_011CStream2","model":"claude-opus-5-5","role":"assistant","content":[{"type":"text","text":"Two runs failed on the lint step."}],"stop_reason":"end_turn","usage":{"input_tokens":4,"cache_creation_input_tokens":120,"cache_read_input_tokens":57000,"output_tokens":480}},"apiBlockIndex":0}"#,
];

fn output_tokens(line: &str) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    value.pointer("/message/usage/output_tokens")?.as_u64()
}

#[test]
fn cost_matches_token_tracker() {
    let fixture = std::fs::read_to_string(FIXTURE).expect("plan 7's recorded streamed transcript");
    let mut text = String::new();
    for line in fixture.lines().chain(STREAMED) {
        text.push_str(line);
        text.push('\n');
    }
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("transcript.jsonl");
    std::fs::write(&path, &text).expect("write the transcript");

    let mut reference = AgentState::new(AgentKind::Main, path.to_str().expect("utf-8 path"));
    for line in text.lines() {
        reference.ingest(line);
    }
    let total = &reference.total;
    let every_line: u64 = text.lines().filter_map(output_tokens).sum();
    assert!(
        every_line > total.output,
        "the transcript streams messages over several lines, so summing every line overcounts"
    );

    let tokens = transcript_tokens(&path).expect("transcript_tokens reads the transcript");
    assert_eq!(
        (
            tokens.fresh_input,
            tokens.cache_write,
            tokens.cache_read,
            tokens.output
        ),
        (
            Some(total.input),
            Some(total.cache_write),
            Some(total.cache_read),
            Some(total.output)
        ),
        "the cost of a transcript equals what the token tracker (loom status) counts"
    );
}

fn document_line(id: &str, source_bytes: u64) -> CostLine {
    CostLine {
        time: "2026-10-28T09:00:00Z".to_string(),
        trigger: Trigger::Background,
        summaries: vec![id.to_string()],
        level: Level::Document,
        model: Some("claude-sonnet-5-5".to_string()),
        source_bytes: Some(source_bytes),
        // Fresh tokens are input + cache writes + output: 12_000. Cache reads never count.
        tokens: TokenVector {
            fresh_input: Some(2_000),
            cache_write: Some(4_000),
            cache_read: Some(30_000),
            output: Some(6_000),
        },
    }
}

fn tokens_for(per_summary: &[(String, u64)], id: &str) -> Option<u64> {
    per_summary.iter().find(|(s, _)| s == id).map(|(_, t)| *t)
}

#[test]
fn estimate_uses_log_or_starting_figure() {
    let empty = estimate(&[], &[("plans/x".to_string(), Level::Document, 10_000)]);
    assert_eq!(
        empty.tokens, 40_000,
        "no log line: the document starting figure"
    );
    assert_eq!(empty.per_summary, vec![("plans/x".to_string(), 40_000)]);
    assert!(matches!(empty.basis, EstimateBasis::Starting));

    let log = [document_line("plans/a", 6_000)];
    let pending = [
        ("plans/a".to_string(), Level::Document, 6_000),
        ("plans/b".to_string(), Level::Document, 3_000),
    ];
    let from_log = estimate(&log, &pending);
    assert_eq!(
        tokens_for(&from_log.per_summary, "plans/a"),
        Some(12_000),
        "plans/a costs what it cost last time"
    );
    assert_eq!(
        tokens_for(&from_log.per_summary, "plans/b"),
        Some(6_000),
        "plans/b has no line of its own: 2 tokens per byte (the median) times 3_000 bytes"
    );
    assert_eq!(from_log.tokens, 18_000);
    assert!(matches!(from_log.basis, EstimateBasis::PerByte));

    // A grown source still estimates at the summary's last cost, not the per-byte figure.
    let grown = estimate(&log, &[("plans/a".to_string(), Level::Document, 9_000)]);
    assert_eq!(
        grown.tokens, 12_000,
        "the last cost wins over 2 x 9_000 bytes"
    );
    assert!(matches!(grown.basis, EstimateBasis::Last));
}
