//! The line-at-a-time fold agrees with the whole-file classifier.

use std::path::PathBuf;

use chrono::Utc;

use super::*;

const FIXTURES: [&str; 4] = [
    "p1-background-agent-agent-1.jsonl",
    "p1-interrupt-agent-1.jsonl",
    "p1-status-agent-1.jsonl",
    "subagent-agent.jsonl",
];

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/plan_session/transcripts")
        .join(name)
}

#[test]
fn folding_observe_over_a_transcript_matches_analyze() {
    for name in FIXTURES {
        let path = fixture(name);
        let mut progress = AgentProgress::default();
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            if let Ok(row) = serde_json::from_str::<Value>(line) {
                progress.observe(&row);
            }
        }
        for debounce in [DEFAULT_DONE_DEBOUNCE_SECS, 0, u64::MAX / 2] {
            let summary = analyze(&path, "agent-1".into(), debounce, None).unwrap();
            assert_eq!(
                progress.state(Utc::now(), debounce),
                summary.state,
                "{name} state at debounce {debounce}"
            );
        }
        let summary = analyze(&path, "agent-1".into(), DEFAULT_DONE_DEBOUNCE_SECS, None).unwrap();
        assert_eq!(progress.assistant_entries(), summary.turns, "{name} turns");
        assert_eq!(
            progress.last_tool(),
            summary.last_tool.as_deref(),
            "{name} last tool"
        );
    }
}
