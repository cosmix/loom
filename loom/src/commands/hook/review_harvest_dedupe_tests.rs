//! One recorded round per reviewer agent per reviewed state: SubagentStop can
//! fire twice for one reviewer, and the second harvest must not re-record it.

use super::*;

/// The outcome of a harvest that wrote round `round` from a clean report.
fn recorded(round: u32) -> Harvest {
    Harvest::Recorded {
        round,
        malformed: None,
        unjournaled: None,
        discrepancy: None,
    }
}

fn suggestion_count(fixture: &Fixture) -> usize {
    read_journal(&fixture.work_dir, STAGE)
        .unwrap()
        .entries
        .iter()
        .filter(|entry| entry.entry_type == MemoryEntryType::Suggestion)
        .count()
}

fn round_count(fixture: &Fixture) -> usize {
    store::load_rounds(&fixture.work_dir, STAGE).unwrap().len()
}

#[test]
fn a_repeated_stop_records_the_report_once() {
    let fixture = fixture(2, REVIEW);

    assert_eq!(
        harvest(&fixture.work_dir, &fixture.input).unwrap(),
        recorded(1)
    );
    let again = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(again, Harvest::Duplicate { round: 1 });
    assert_eq!(round_count(&fixture), 1);
    assert!(!reviews_dir(&fixture).join("round-2.json").exists());
    assert_eq!(
        suggestion_count(&fixture),
        1,
        "a duplicate journals nothing"
    );
}

#[test]
fn a_repeated_malformed_stop_records_one_round() {
    let fixture = fixture(2, "Looks fine to me.");

    harvest(&fixture.work_dir, &fixture.input).unwrap();
    let again = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(again, Harvest::Duplicate { round: 1 });
    assert_eq!(round_count(&fixture), 1);
}

#[test]
fn the_same_agent_with_different_findings_records_a_new_round() {
    let fixture = fixture(2, REVIEW);
    harvest(&fixture.work_dir, &fixture.input).unwrap();
    write_transcript(&fixture.input.transcript_path, REVIEW_ALT);

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(outcome, recorded(2));
    let rounds = store::load_rounds(&fixture.work_dir, STAGE).unwrap();
    assert_eq!(rounds[0].findings[0].id, "F-1-1");
    assert_eq!(rounds[1].findings[0].id, "F-2-1");
    assert_eq!(rounds[1].findings[0].finding.claim, "b() ignores errors");
}

#[test]
fn a_replayed_stop_cannot_certify_edits_after_the_review() {
    let fixture = fixture(2, REVIEW);
    harvest(&fixture.work_dir, &fixture.input).unwrap();
    fs::write(fixture.worktree.join("changed.txt"), "after the review\n").unwrap();

    assert!(harvest(&fixture.work_dir, &fixture.input).is_err());
    assert_eq!(round_count(&fixture), 1);
    let rounds = store::load_rounds(&fixture.work_dir, STAGE).unwrap();
    assert_ne!(
        rounds[0].fingerprint,
        fingerprint::compute(&fixture.worktree, "main")
            .unwrap()
            .value
    );
}

#[test]
fn a_repeated_start_cannot_rebind_an_agent_after_edits() {
    let fixture = fixture(2, REVIEW);
    fs::write(fixture.worktree.join("changed.txt"), "after the start\n").unwrap();

    assert!(
        crate::commands::hook::review_start::capture(&fixture.work_dir, &fixture.input).is_err()
    );
    assert!(harvest(&fixture.work_dir, &fixture.input).is_err());
    assert_eq!(round_count(&fixture), 0);
}

#[test]
fn a_new_reviewer_invocation_can_review_the_edited_tree() {
    let mut fixture = fixture(2, REVIEW);
    harvest(&fixture.work_dir, &fixture.input).unwrap();
    fs::write(fixture.worktree.join("changed.txt"), "after the review\n").unwrap();
    let other = fixture
        .input
        .transcript_path
        .with_file_name("agent-other.jsonl");
    fs::copy(&fixture.input.transcript_path, &other).unwrap();
    fixture.input.agent_id = "other".to_string();
    fixture.input.transcript_path = other;
    crate::commands::hook::review_start::capture(&fixture.work_dir, &fixture.input).unwrap();

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(outcome, recorded(2));
    assert_eq!(round_count(&fixture), 2);
}

#[test]
fn different_agents_with_the_same_report_record_two_rounds() {
    let mut fixture = fixture(2, REVIEW);
    harvest(&fixture.work_dir, &fixture.input).unwrap();
    let other = fixture
        .input
        .transcript_path
        .with_file_name("agent-other.jsonl");
    fs::copy(&fixture.input.transcript_path, &other).unwrap();
    fixture.input.agent_id = "other".to_string();
    fixture.input.transcript_path = other;
    crate::commands::hook::review_start::capture(&fixture.work_dir, &fixture.input).unwrap();

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(outcome, recorded(2));
    assert_eq!(round_count(&fixture), 2);
    assert_eq!(suggestion_count(&fixture), 2);
}
