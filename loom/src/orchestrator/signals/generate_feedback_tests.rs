//! Tests for the adjudicator feedback `append_stage_feedback` renders, split
//! out of `generate.rs` to keep that file under the size ceiling.

use super::*;
use crate::orchestrator::adjudication::feedback::append_questions;

const HEADING: &str = "## Adjudicator Feedback (from your prior dispute)";

/// The stage's rendered feedback, with feedback waiting on disk.
fn rendered(stage: &Stage) -> String {
    let temp = tempfile::tempdir().unwrap();
    append_questions(
        temp.path(),
        &stage.id,
        &["Which test covers it?".to_string()],
    )
    .unwrap();
    let mut content = String::new();
    append_stage_feedback(&mut content, stage, temp.path());
    content
}

#[test]
fn feedback_renders_after_a_dispute_of_any_kind() {
    let counters: [fn(&mut Stage); 4] = [
        |stage| stage.dispute_count = 1,
        |stage| stage.tally.finding_disputes = 1,
        |stage| stage.tally.contract_disputes = 1,
        |stage| stage.tally.integrity_disputes = 1,
    ];
    for (i, set) in counters.iter().enumerate() {
        let mut stage = Stage {
            id: "s1".to_string(),
            ..Stage::default()
        };
        set(&mut stage);
        let content = rendered(&stage);
        assert!(content.contains(HEADING), "counter {i}: {content}");
        assert!(content.contains("Which test covers it?"), "counter {i}");
    }
}

#[test]
fn feedback_stays_out_of_an_undisputed_stage() {
    let stage = Stage {
        id: "s1".to_string(),
        ..Stage::default()
    };
    assert!(!rendered(&stage).contains(HEADING));
}
