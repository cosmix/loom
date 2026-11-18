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

    // The repeat stop is the report round 1 holds, so it is no failure; it
    // still writes nothing, so round 1 stays bound to the tree it reviewed.
    assert_eq!(
        harvest(&fixture.work_dir, &fixture.input).unwrap(),
        Harvest::Duplicate { round: 1 }
    );
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
fn a_different_report_after_edits_is_refused_as_a_changed_worktree() {
    let fixture = fixture(2, REVIEW);
    harvest(&fixture.work_dir, &fixture.input).unwrap();
    fs::write(fixture.worktree.join("changed.txt"), "after the review\n").unwrap();
    write_transcript(&fixture.input.transcript_path, REVIEW_ALT);

    let error = harvest(&fixture.work_dir, &fixture.input).unwrap_err();

    assert_eq!(failure_cause(&error), Some("worktree_changed"));
    assert!(
        format!("{error:#}").contains("launch a new review"),
        "{error:#}"
    );
    assert_eq!(round_count(&fixture), 1);
}

#[test]
fn a_harvest_failure_names_its_cause() {
    let mut fixture = fixture(2, REVIEW);
    fixture.input.session_id = "other-parent".to_string();
    let error = harvest(&fixture.work_dir, &fixture.input).unwrap_err();
    assert_eq!(failure_cause(&error), Some("transcript_unusable"));

    let fixture = super::fixture(2, REVIEW);
    let start_file = fs::read_dir(reviews_dir(&fixture))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.to_string_lossy().contains("start-"))
        .unwrap();
    fs::remove_file(start_file).unwrap();
    let error = harvest(&fixture.work_dir, &fixture.input).unwrap_err();
    assert_eq!(failure_cause(&error), Some("no_start_evidence"));
    assert!(
        format!("{error:#}").contains("reviewer start evidence"),
        "{error:#}"
    );

    let fixture = super::fixture(2, REVIEW);
    fs::remove_dir_all(&fixture.worktree).unwrap();
    let error = harvest(&fixture.work_dir, &fixture.input).unwrap_err();
    assert_eq!(failure_cause(&error), None, "{error:#}");
}

#[test]
fn a_repeated_start_cannot_rebind_an_agent_after_edits() {
    let fixture = fixture(2, REVIEW);
    fs::write(fixture.worktree.join("changed.txt"), "after the start\n").unwrap();

    assert!(crate::commands::hook::review_start::capture(
        &fixture.work_dir,
        &fixture.input.invocation()
    )
    .is_err());
    assert!(harvest(&fixture.work_dir, &fixture.input).is_err());
    assert_eq!(round_count(&fixture), 0);
}

/// The start snapshot, not the tree at SubagentStop, binds the harvest: the
/// tree the reviewer read must match even though the start named no transcript.
#[test]
fn start_evidence_binds_each_agent_to_its_own_start_tree() {
    let mut fixture = fixture(2, REVIEW);
    fs::write(fixture.worktree.join("changed.txt"), "after the first\n").unwrap();
    fixture.input.agent_id = "other".to_string();
    fixture.input.transcript_path = fixture
        .input
        .transcript_path
        .with_file_name("agent-other.jsonl");
    crate::commands::hook::review_start::capture(&fixture.work_dir, &fixture.input.invocation())
        .unwrap();
    write_transcript(&fixture.input.transcript_path, REVIEW);

    assert_eq!(
        harvest(&fixture.work_dir, &fixture.input).unwrap(),
        recorded(1)
    );

    fixture.input.agent_id = AGENT.to_string();
    fixture.input.transcript_path = fixture
        .input
        .transcript_path
        .with_file_name(format!("agent-{AGENT}.jsonl"));
    assert!(
        harvest(&fixture.work_dir, &fixture.input).is_err(),
        "the first agent started on the earlier tree"
    );
    assert_eq!(round_count(&fixture), 1);
}

/// A session id is part of the invocation: the same agent id under another
/// parent session has no start evidence, even with its own valid transcript.
#[test]
fn start_evidence_is_keyed_on_the_parent_session() {
    let mut fixture = fixture(2, REVIEW);
    let elsewhere = fixture
        .input
        .transcript_path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .with_file_name("other-parent")
        .join("subagents")
        .join(format!("agent-{AGENT}.jsonl"));
    write_transcript(&elsewhere, REVIEW);
    fixture.input.session_id = "other-parent".to_string();
    fixture.input.transcript_path = elsewhere;

    let error = harvest(&fixture.work_dir, &fixture.input).unwrap_err();

    assert!(
        format!("{error:#}").contains("reviewer start evidence"),
        "{error:#}"
    );
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
    crate::commands::hook::review_start::capture(&fixture.work_dir, &fixture.input.invocation())
        .unwrap();

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
    crate::commands::hook::review_start::capture(&fixture.work_dir, &fixture.input.invocation())
        .unwrap();

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(outcome, recorded(2));
    assert_eq!(round_count(&fixture), 2);
    assert_eq!(suggestion_count(&fixture), 2);
}
