use super::*;

use std::fs;

use serde_json::{json, Value};
use tempfile::TempDir;

use crate::fs::memory::{memory_file_path, read_journal};
use crate::verify::contracts::test_support::{contract_worktree, CONTRACT_FILE};
use crate::verify::transitions::create_stage;

const STAGE: &str = "review-stage";
const PARENT: &str = "parent-session";
const AGENT: &str = "a1b2c3";

const REVIEW: &str = "One finding.\n\n```loom-review\n\
    {\"findings\":[{\"severity\":\"major\",\"file\":\"a.rs\",\"line\":1,\
    \"claim\":\"a() lost its body\",\"scenario\":\"calling a() does nothing\",\"rule\":null}],\
    \"suggestions\":[{\"file\":\"a.rs\",\"line\":2,\"text\":\"name the\\nhelper\"}],\
    \"resolved\":[],\"unresolved\":[]}\n```\n";

/// A second well-formed block, distinct from `REVIEW`, for the tests that
/// need two candidates which parse but disagree.
const REVIEW_ALT: &str = "A different finding.\n\n```loom-review\n\
    {\"findings\":[{\"severity\":\"minor\",\"file\":\"b.rs\",\"line\":9,\
    \"claim\":\"b() ignores errors\",\"scenario\":\"call fails silently\",\"rule\":null}],\
    \"suggestions\":[],\"resolved\":[],\"unresolved\":[]}\n```\n";

/// Larger than `review_transcript::TRANSCRIPT_TAIL_BYTES`, so a row padded to
/// this length forces `read_tail`'s window to start inside it.
const PADDING_BYTES: usize = 5 * 1024 * 1024;

struct Fixture {
    _temp: TempDir,
    work_dir: PathBuf,
    worktree: PathBuf,
    input: HarvestInput,
}

/// A project on `main` whose stage worktree holds one untracked file, the
/// stage record at `plan_version`, and a reviewer transcript whose final
/// assistant message is `final_text`.
fn fixture(plan_version: u32, final_text: &str) -> Fixture {
    fixture_with(plan_version, |path| write_transcript(path, final_text))
}

/// As `fixture`, but `write` builds the transcript file itself.
fn fixture_with(plan_version: u32, write: impl FnOnce(&Path)) -> Fixture {
    let temp = TempDir::new().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let project = root.join("project");
    let worktree = contract_worktree(&project, STAGE);

    let work_dir = project.join(".loom").join("work");
    let stage = Stage {
        id: STAGE.to_string(),
        name: STAGE.to_string(),
        worktree: Some(STAGE.to_string()),
        plan_version,
        ..Stage::default()
    };
    create_stage(&stage, &work_dir).unwrap();

    let transcript = root
        .join("claude")
        .join(PARENT)
        .join("subagents")
        .join(format!("agent-{AGENT}.jsonl"));
    write(&transcript);
    let input = HarvestInput {
        stage_id: STAGE.to_string(),
        session_id: PARENT.to_string(),
        agent_id: AGENT.to_string(),
        transcript_path: transcript,
    };
    Fixture {
        _temp: temp,
        work_dir,
        worktree,
        input,
    }
}

fn write_transcript(path: &Path, final_text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let assistant = |block: Value| json!({"type": "assistant", "message": {"content": [block]}});
    let rows = [
        json!({"type": "user", "message": {"content": "review the stage"}}),
        assistant(json!({"type": "text", "text": final_text})),
        assistant(json!({"type": "thinking", "thinking": "done"})),
    ];
    let body: String = rows.iter().map(|row| format!("{row}\n")).collect();
    fs::write(path, body).unwrap();
}

/// An assistant entry with only a `SubagentHandback` tool call whose
/// `message` input is `handback_message`, followed by a final assistant text
/// entry: the shape a real reviewer transcript takes today.
fn write_handback_transcript(path: &Path, handback_message: &str, final_text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let assistant = |block: Value| json!({"type": "assistant", "message": {"content": [block]}});
    let rows = [
        json!({"type": "user", "message": {"content": "review the stage"}}),
        assistant(handback_block(handback_message)),
        assistant(json!({"type": "text", "text": final_text})),
    ];
    let body: String = rows.iter().map(|row| format!("{row}\n")).collect();
    fs::write(path, body).unwrap();
}

/// A `SubagentHandback` tool call sitting in a `user` entry instead of an
/// assistant one, as a tool result quoting it back would produce.
fn write_transcript_with_handback_in_user_entry(
    path: &Path,
    handback_message: &str,
    final_text: &str,
) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let assistant = |block: Value| json!({"type": "assistant", "message": {"content": [block]}});
    let rows = [
        json!({"type": "user", "message": {"content": [handback_block(handback_message)]}}),
        assistant(json!({"type": "text", "text": final_text})),
    ];
    let body: String = rows.iter().map(|row| format!("{row}\n")).collect();
    fs::write(path, body).unwrap();
}

fn handback_block(message: &str) -> Value {
    json!({
        "type": "tool_use",
        "id": "toolu_handback",
        "name": "SubagentHandback",
        "input": {"message": message},
    })
}

/// A transcript whose real rows are preceded by a row padded past the tail
/// window, so `read_tail` must seek into the middle of it and drop the
/// fragment.
fn write_padded_transcript(path: &Path, final_text: &str) {
    write_transcript(path, final_text);
    let body = fs::read_to_string(path).unwrap();
    let padding = "#".repeat(PADDING_BYTES);
    fs::write(path, format!("{padding}\n{body}")).unwrap();
}

fn reviews_dir(fixture: &Fixture) -> PathBuf {
    fixture.work_dir.join("reviews").join(STAGE)
}

#[test]
fn harvest_writes_round_and_suggestions() {
    let fixture = fixture(2, REVIEW);

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    let expected = Harvest::Recorded {
        round: 1,
        malformed: None,
        unjournaled: None,
        discrepancy: None,
    };
    assert_eq!(outcome, expected);
    assert!(reviews_dir(&fixture).join("round-1.json").is_file());
    let rounds = store::load_rounds(&fixture.work_dir, STAGE).unwrap();
    let round = &rounds[0];
    assert_eq!(rounds.len(), 1);
    assert_eq!(round.agent_id, AGENT);
    assert_eq!(round.findings.len(), 1);
    assert_eq!(round.findings[0].id, "F-1-1");
    assert_eq!(round.findings[0].finding.claim, "a() lost its body");
    let current = fingerprint::compute(&fixture.worktree, "main").unwrap();
    assert_eq!(round.fingerprint, current.value);
    assert_eq!(round.files, current.files);
    assert!(round.files.contains_key(CONTRACT_FILE));

    let journal = read_journal(&fixture.work_dir, STAGE).unwrap();
    let suggestions: Vec<&MemoryEntry> = journal
        .entries
        .iter()
        .filter(|entry| entry.entry_type == MemoryEntryType::Suggestion)
        .collect();
    assert_eq!(suggestions.len(), 1);
    let suggestion = suggestions[0];
    assert_eq!(suggestion.content, "a.rs:2 name the helper");
    assert_eq!(suggestion.evidence, ["review round 1"]);
    assert_eq!(
        round.suggestion_memory_ids,
        std::slice::from_ref(&suggestion.id)
    );
    let settled = journal.entries.iter().any(|entry| {
        entry
            .receipt
            .as_ref()
            .is_some_and(|receipt| receipt.event_id == suggestion.id)
    });
    assert!(!settled, "a freshly harvested suggestion is pending");
}

#[test]
fn harvest_skips_v1_stage() {
    let fixture = fixture(1, REVIEW);

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    assert_eq!(outcome, Harvest::Skipped);
    assert!(!fixture.work_dir.join("reviews").exists());
    assert!(!memory_file_path(&fixture.work_dir, STAGE).exists());
}

#[test]
fn harvest_records_a_malformed_round_without_a_review_block() {
    let fixture = fixture(2, "Looks fine to me.");

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    let expected = Harvest::Recorded {
        round: 1,
        malformed: Some("no loom-review block".to_string()),
        unjournaled: None,
        discrepancy: None,
    };
    assert_eq!(outcome, expected);
    let rounds = store::load_rounds(&fixture.work_dir, STAGE).unwrap();
    assert!(rounds[0].findings.is_empty());
    assert!(rounds[0].suggestion_memory_ids.is_empty());
    assert!(!memory_file_path(&fixture.work_dir, STAGE).exists());
}

#[test]
fn harvest_records_only_the_suggestion_ids_it_journaled() {
    let fixture = fixture(2, REVIEW);
    // A directory where the journal file belongs makes every append fail.
    fs::create_dir_all(memory_file_path(&fixture.work_dir, STAGE)).unwrap();

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    let reason = match outcome {
        Harvest::Recorded {
            round: 1,
            malformed: None,
            unjournaled: Some(reason),
            discrepancy: None,
        } => reason,
        other => panic!("expected a round with unjournaled suggestions: {other:?}"),
    };
    assert!(reason.contains("journaled 0 of 1 suggestions"), "{reason}");
    let rounds = store::load_rounds(&fixture.work_dir, STAGE).unwrap();
    assert_eq!(rounds[0].findings.len(), 1);
    assert!(rounds[0].suggestion_memory_ids.is_empty());
}

#[test]
fn harvest_rejects_a_transcript_of_another_agent() {
    let mut fixture = fixture(2, REVIEW);
    fixture.input.agent_id = "other-agent".to_string();

    assert!(harvest(&fixture.work_dir, &fixture.input).is_err());
    assert!(!fixture.work_dir.join("reviews").exists());
}

#[test]
fn harvest_rejects_a_symlinked_transcript() {
    let mut fixture = fixture(2, REVIEW);
    let link = fixture
        .input
        .transcript_path
        .with_file_name("agent-linked.jsonl");
    std::os::unix::fs::symlink(&fixture.input.transcript_path, &link).unwrap();
    fixture.input.agent_id = "linked".to_string();
    fixture.input.transcript_path = link;

    assert!(harvest(&fixture.work_dir, &fixture.input).is_err());
    assert!(!fixture.work_dir.join("reviews").exists());
}

#[test]
fn harvest_reads_the_report_from_a_handback_tool_call() {
    let fixture = fixture_with(2, |path| {
        write_handback_transcript(path, REVIEW, "Report delivered.")
    });

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    let expected = Harvest::Recorded {
        round: 1,
        malformed: None,
        unjournaled: None,
        discrepancy: None,
    };
    assert_eq!(outcome, expected);
    let rounds = store::load_rounds(&fixture.work_dir, STAGE).unwrap();
    assert_eq!(rounds[0].findings[0].finding.claim, "a() lost its body");
}

#[test]
fn harvest_prefers_the_handback_over_a_disagreeing_final_text() {
    let fixture = fixture_with(2, |path| {
        write_handback_transcript(path, REVIEW, REVIEW_ALT)
    });

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    let discrepancy = match outcome {
        Harvest::Recorded {
            round: 1,
            malformed: None,
            unjournaled: None,
            discrepancy: Some(reason),
        } => reason,
        other => panic!("expected a round with a discrepancy: {other:?}"),
    };
    assert!(discrepancy.contains("disagree"), "{discrepancy}");
    let rounds = store::load_rounds(&fixture.work_dir, STAGE).unwrap();
    // The hand-back's finding, not the final text's ("b() ignores errors").
    assert_eq!(rounds[0].findings[0].finding.claim, "a() lost its body");
}

#[test]
fn harvest_ignores_a_handback_block_outside_an_assistant_entry() {
    let fixture = fixture_with(2, |path| {
        write_transcript_with_handback_in_user_entry(path, REVIEW, "Report delivered.")
    });

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    let expected = Harvest::Recorded {
        round: 1,
        malformed: Some("no loom-review block".to_string()),
        unjournaled: None,
        discrepancy: None,
    };
    assert_eq!(outcome, expected);
}

#[test]
fn harvest_parses_a_tail_that_starts_mid_row() {
    let fixture = fixture_with(2, |path| write_padded_transcript(path, REVIEW));

    let outcome = harvest(&fixture.work_dir, &fixture.input).unwrap();

    let expected = Harvest::Recorded {
        round: 1,
        malformed: None,
        unjournaled: None,
        discrepancy: None,
    };
    assert_eq!(outcome, expected);
}
