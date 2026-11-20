//! The contract briefing of a dispute that carries a replacement for the
//! frozen file: it is reviewed by reading, never presented as a run result.

use super::tests::{contract_fixture, freeze, input_of, ContractFixture};
use super::*;
use crate::models::dispute::FileReplacement;

const FROZEN: &str = "#[test]\nfn rejects_x() { assert!(rejects(\"x\")); }\n";
/// Does not compile: judging it by running anything would be a mistake.
const BROKEN_REPLACEMENT: &str = "#[test]\nfn rejects_x( { assert!(;\n";

/// A fixture whose frozen test passes and whose worktree file is unmodified,
/// disputing with `replacement`.
fn fixture_with(replacement: Option<&str>) -> ContractFixture {
    let mut fx = contract_fixture();
    freeze(&fx.work_dir, &[("tests/x_contract.rs", FROZEN)]);
    fs::write(fx.site.path.join("tests/x_contract.rs"), FROZEN).unwrap();
    fx.request.kind = DisputeKind::Contract {
        contract_ids: vec!["rejects-x".to_string()],
        replacements: replacement
            .map(|content| FileReplacement {
                file: "tests/x_contract.rs".to_string(),
                content: content.to_string(),
                base_sha256: None,
            })
            .into_iter()
            .collect(),
    };
    fx
}

fn headings(evidence: &str) -> Vec<&str> {
    evidence
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect()
}

#[test]
fn the_replacement_is_quoted_and_diffed_against_the_frozen_file() {
    let fx = fixture_with(Some(BROKEN_REPLACEMENT));

    let prompt = build(&input_of(&fx));

    assert!(prompt.evidence.contains(&format!(
        "## Proposed replacement\n\nFile: `tests/x_contract.rs` (contracts: rejects-x)\n\n\
         Trial: no matching trial receipt is still recorded\n\n```text\n{BROKEN_REPLACEMENT}```\n"
    )));
    let diff_at = prompt
        .evidence
        .find("## Diff (frozen vs replacement)")
        .expect("a replacement diff section");
    let diff = &prompt.evidence[diff_at..];
    assert!(
        diff.contains("-#[test]\n") || diff.contains("-fn rejects_x() {"),
        "{diff}"
    );
    assert!(diff.contains("+fn rejects_x( { assert!(;"), "{diff}");
}

#[test]
fn without_a_replacement_the_briefing_has_no_replacement_parts() {
    let fx = fixture_with(None);

    let prompt = build(&input_of(&fx));

    assert_eq!(
        headings(&prompt.evidence),
        [
            "## Dispute",
            "## Contract spec",
            "## Frozen content",
            "## Agent's reason",
            "## Other frozen files changed since the freeze",
        ]
    );
    for absent in [
        "replacement has not been run",
        "proposed replacement",
        "writes the replacement into the worktree",
    ] {
        assert!(!prompt.instructions.contains(absent), "{absent}");
    }
    assert!(prompt
        .instructions
        .contains("contract, frozen file, or diff. A citation has file, optional"));
}

#[test]
fn every_contract_prompt_forbids_asking_for_an_edit_of_a_frozen_file() {
    for replacement in [None, Some(BROKEN_REPLACEMENT)] {
        let prompt = build(&input_of(&fixture_with(replacement)));
        assert!(prompt.instructions.contains(
            "Never ask the agent to edit a frozen file; ask only for evidence it can produce \
             read-only, or for a revised `--replacement`."
        ));
    }
}

/// The frozen test passes and the replacement does not compile: the command
/// the prompt shows runs the frozen file, so the prompt must say the
/// replacement was not run and must not present that run as its evidence.
#[test]
fn a_replacement_is_judged_by_reading_not_by_the_command_that_runs_the_frozen_file() {
    let fx = fixture_with(Some(BROKEN_REPLACEMENT));

    let prompt = build(&input_of(&fx));

    let instructions = &prompt.instructions;
    assert!(instructions.contains(
        "The commands above run the frozen content, not a replacement; its trial result is the \
         `Trial:` line under `## Proposed replacement`. Judge each replacement by reading it \
         against the scenario and rejects clause of the contracts in its file. Completion runs \
         it after an accept."
    ));
    assert!(instructions.contains(
        "accept: the contract's replacement, or the `plan_patch` you give for it, still sets up \
         its `scenario` and still fails on its `rejects` implementation."
    ));
    assert!(instructions.contains("An accepted replacement is written into the worktree"));
    assert!(instructions
        .contains("contract, frozen file, proposed replacement, or diff. A citation has file"));
    for claim in [
        "the replacement passes",
        "passes with the replacement",
        "replacement run",
    ] {
        assert!(!instructions.contains(claim), "{claim}");
    }
}

/// A replacement that weakens an assertion lists the frozen statement the
/// judge must find covered, and the verdict semantics ask for that citation.
#[test]
fn the_assertions_a_replacement_drops_are_listed_and_must_be_covered() {
    let weakened = "#[test]\nfn rejects_x() { assert!(true); }\n";
    let prompt = build(&input_of(&fixture_with(Some(weakened))));

    let section = prompt
        .evidence
        .split("### Assertions the replacement drops or changes")
        .nth(1)
        .expect("a lost-assertions section");
    assert!(section.contains("File: `tests/x_contract.rs`"), "{section}");
    assert!(section.contains("assert!(rejects(\"x\"))"), "{section}");
    assert!(prompt.instructions.contains(
        "cite what in the replacement covers the same behaviour, or reject the contracts of \
         that file"
    ));

    let kept = build(&input_of(&fixture_with(Some(
        "#[test]\nfn rejects_x() { assert!(rejects(\"x\")); }\nfn helper() {}\n",
    ))));
    let section = kept
        .evidence
        .split("### Assertions the replacement drops or changes")
        .nth(1)
        .expect("a lost-assertions section");
    assert!(section.contains("\nnone\n"), "{section}");
}

/// The trial line names the commit the recorded passing trial ran at.
#[test]
fn the_proposed_replacement_names_the_trial_that_passed() {
    let fx = fixture_with(Some(BROKEN_REPLACEMENT));
    assert!(!build(&input_of(&fx)).evidence.contains("Trial: passed"));
    save_passing_trial(&fx.work_dir, "demo", "abc123", BROKEN_REPLACEMENT);

    let prompt = build(&input_of(&fx));

    assert!(prompt.evidence.contains(
        "Trial: passed in a confined host job at abc123 (format and static gates, then the \
         contracts)"
    ));
    assert!(!prompt.evidence.contains("no matching trial receipt"));
}

/// Without a receipt (never run, or pruned) the line says so, since the
/// first step tells the judge to read it.
#[test]
fn without_a_receipt_the_trial_line_says_none_is_recorded() {
    let fx = fixture_with(Some(BROKEN_REPLACEMENT));

    let prompt = build(&input_of(&fx));

    assert!(prompt
        .evidence
        .contains("Trial: no matching trial receipt is still recorded"));
    assert!(!prompt.evidence.contains("Trial: passed"));
}

/// A receipt of other bytes for the file proves nothing for this filing.
#[test]
fn a_receipt_of_other_bytes_is_no_trial_of_this_filing() {
    let fx = fixture_with(Some(BROKEN_REPLACEMENT));
    save_passing_trial(&fx.work_dir, "demo", "abc123", "an earlier replacement\n");

    let prompt = build(&input_of(&fx));

    assert!(prompt
        .evidence
        .contains("Trial: no matching trial receipt is still recorded"));
    assert!(!prompt.evidence.contains("Trial: passed"));
}

/// Leave a passing contracts check of stage `stage_id` at `head` and its
/// receipt, which hashed `content` for the disputed file.
fn save_passing_trial(work_dir: &Path, stage_id: &str, head: &str, content: &str) {
    use crate::host_verify::{new_job_id, store, JobKind, JobRecord, JobResult, JobState};
    use crate::verify::contracts::trial::{TrialFile, TrialReceipt, RECEIPT_FILE};
    let record = JobRecord {
        id: new_job_id(),
        submission_order: 1,
        stage_id: stage_id.to_string(),
        session_id: "session-1".to_string(),
        kind: JobKind::Check,
        head: head.to_string(),
        tree_digest: "digest".to_string(),
        no_cache: false,
        criteria: None,
        contracts: true,
        at_base: false,
        state: JobState::Passed,
        requests: Vec::new(),
        queued_at: chrono::Utc::now(),
        started_at: None,
        finished_at: None,
        pid: None,
        detail: None,
    };
    store::save_current(work_dir, &record).unwrap();
    let result = JobResult {
        job_id: record.id.clone(),
        head: head.to_string(),
        passed: true,
        criteria: Vec::new(),
        error: None,
        policy_hash: None,
    };
    store::save_result(work_dir, stage_id, &result).unwrap();
    let receipt = TrialReceipt {
        head: head.to_string(),
        files: vec![TrialFile {
            path: "tests/x_contract.rs".to_string(),
            sha256: crate::utils::sha256_hex(content.as_bytes()),
        }],
    };
    let bytes = serde_json::to_vec(&receipt).unwrap();
    store::save_job_file(work_dir, stage_id, &record.id, RECEIPT_FILE, &bytes).unwrap();
}
