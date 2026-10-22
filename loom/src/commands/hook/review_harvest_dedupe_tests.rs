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
fn the_same_agent_at_a_different_fingerprint_records_a_new_round() {
    let fixture = fixture(2, REVIEW);
    harvest(&fixture.work_dir, &fixture.input).unwrap();
    fs::write(fixture.worktree.join("changed.txt"), "after the review\n").unwrap();

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

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(outcome, recorded(2));
    assert_eq!(round_count(&fixture), 2);
    assert_eq!(suggestion_count(&fixture), 2);
}

#[test]
fn concurrent_stops_of_one_agent_record_one_round() {
    let fixture = fixture(2, REVIEW);

    let outcomes: Vec<Harvest> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| harvest(&fixture.work_dir, &fixture.input).unwrap()))
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });

    let written = outcomes.iter().filter(|o| **o == recorded(1)).count();
    let duplicates = outcomes
        .iter()
        .filter(|o| **o == Harvest::Duplicate { round: 1 })
        .count();
    assert_eq!((written, duplicates), (1, 3), "{outcomes:?}");
    assert_eq!(round_count(&fixture), 1);
    assert_eq!(suggestion_count(&fixture), 1);
}
