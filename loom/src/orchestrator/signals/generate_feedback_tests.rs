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
    let counters: [fn(&mut Stage); 5] = [
        |stage| stage.dispute_count = 1,
        |stage| stage.tally.finding_disputes = 1,
        |stage| stage.tally.contract_disputes = 1,
        |stage| stage.tally.integrity_disputes = 1,
        |stage| stage.tally.impact_disputes = 1,
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

#[test]
fn a_handback_refusal_renders_without_any_dispute() {
    let mut stage = Stage {
        id: "s1".to_string(),
        ..Stage::default()
    };
    stage.merge.handback_refusal = Some("first failure\n\nsecond `failure`\n".to_string());
    let temp = tempfile::tempdir().unwrap();
    let mut content = String::new();

    append_stage_feedback(&mut content, &stage, temp.path());

    assert!(content.contains("## Merge Resolver Handback"), "{content}");
    assert!(content.contains("- first failure\n"), "{content}");
    assert_eq!(content.matches("\n- ").count(), 2, "{content}");
    assert!(!content.contains(HEADING));
}

#[test]
fn a_stage_without_a_handback_gets_no_handback_section() {
    let stage = Stage {
        id: "s1".to_string(),
        ..Stage::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let mut content = String::new();

    append_stage_feedback(&mut content, &stage, temp.path());

    assert!(!content.contains("Merge Resolver Handback"), "{content}");
}

/// The handback section of a stage whose refusal is `refusal`.
fn handback_section(refusal: &str, plan_version: u32) -> String {
    let mut stage = Stage {
        id: "s1".to_string(),
        plan_version,
        stage_type: crate::models::stage::StageType::Standard,
        ..Stage::default()
    };
    stage.merge.handback_refusal = Some(refusal.to_string());
    crate::orchestrator::signals::section_formatters::format_merge_handback_section(&stage).unwrap()
}

#[test]
fn a_long_refusal_line_appears_whole() {
    let line = format!("failed: {}", "x".repeat(450));

    let content = handback_section(&line, 1);

    assert!(content.contains(&format!("- {line}\n")), "{content}");
}

#[test]
fn a_backtick_and_a_control_character_are_neutralised() {
    let content = handback_section("bad `code` here\u{1b}[31m red", 1);

    let bullet = content.lines().find(|l| l.starts_with("- ")).unwrap();
    assert!(!bullet.contains('\u{1b}'), "{bullet:?}");
    assert!(bullet.chars().all(|c| !c.is_control()), "{bullet:?}");
    assert_eq!(
        bullet,
        format!(
            "- {}",
            crate::context::untrusted::multiline_safe("bad `code` here\u{1b}[31m red")
        )
    );
}

#[test]
fn only_forty_refusal_lines_are_listed() {
    let refusal = (1..=41)
        .map(|n| format!("failure {n}"))
        .collect::<Vec<_>>()
        .join("\n");

    let content = handback_section(&refusal, 1);

    assert_eq!(content.matches("\n- ").count(), 40, "{content}");
    assert!(!content.contains("failure 41"), "{content}");
}

#[test]
fn the_review_sentence_is_only_for_stages_the_review_gate_covers() {
    assert!(handback_section("x", 2).contains("needs a review round that matches the tree"));
    assert!(!handback_section("x", 1).contains("review round"));
    assert!(handback_section("x", 1).contains("run `loom stage complete` again"));
}

#[test]
fn a_recovery_signal_carries_the_handback_refusal() {
    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    let mut stage = Stage {
        id: "s1".to_string(),
        ..Stage::default()
    };
    stage.merge.handback_refusal = Some("tests failed in module x".to_string());
    let content = crate::orchestrator::signals::RecoverySignalContent::for_crash(
        "session-recovered".into(),
        stage.id.clone(),
        "session-crashed".into(),
        None,
        1,
    );

    let path = crate::orchestrator::signals::generate_recovery_signal(&content, &stage, &work_dir)
        .unwrap();

    let signal = std::fs::read_to_string(path).unwrap();
    assert!(signal.contains("## Merge Resolver Handback"), "{signal}");
    assert!(signal.contains("- tests failed in module x\n"), "{signal}");
}
