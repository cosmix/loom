//! Routing tests: each kind of dispute reaches its own builder, and every
//! briefing keeps the byte cap with its instructions whole.

use super::*;
use crate::models::dispute::{FindingSnapshot, IntegritySnapshot};
use crate::models::stage::{SuccessCriteria, WiringCheck, WiringTest};
use crate::plan::schema::AcceptanceCriterion;
use crate::verify::contracts::test_support::{contract, CONTRACT_ID};
use crate::verify::integrity::EventKind;
use crate::verify::review::report::Finding;
use chrono::Utc;

const RECORD_COMMAND: &str = "loom stage adjudicate --stage demo --dispute 4 --verdict-file";

fn stage() -> Stage {
    Stage {
        id: "demo".to_string(),
        plan_version: 2,
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        contracts: vec![contract()],
        ..Stage::default()
    }
}

fn request(kind: DisputeKind, reason: &str) -> DisputeRequest {
    DisputeRequest {
        id: 4,
        stage_id: "demo".to_string(),
        kind,
        reason: reason.to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: Utc::now(),
    }
}

fn findings_kind() -> DisputeKind {
    let snapshot = FindingSnapshot {
        id: "F-1-2".to_string(),
        origin_stage: None,
        round: 1,
        finding: Finding {
            severity: "major".to_string(),
            file: "src/a.rs".to_string(),
            line: 42,
            claim: "unwrap on input".to_string(),
            scenario: Some("an empty input panics".to_string()),
            rule: None,
        },
    };
    DisputeKind::Findings {
        finding_ids: vec![snapshot.id.clone()],
        evidence: vec![snapshot],
    }
}

fn integrity_kind() -> DisputeKind {
    let event = IntegritySnapshot {
        id: "TI-decl-rust".to_string(),
        kind: EventKind::DeclTotal,
        language: Some("rust".to_string()),
        path: None,
        base: Some(10),
        current: Some(8),
        current_sha256: None,
        detail: Vec::new(),
    };
    DisputeKind::Integrity {
        event_ids: vec![event.id.clone()],
        evidence: vec![event],
    }
}

fn contract_kind() -> DisputeKind {
    DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
    }
}

/// The briefing for `kind`, built in a tmp tree with no stage worktree.
fn briefing(kind: DisputeKind, reason: &str) -> Prompt {
    let tmp = tempfile::tempdir().unwrap();
    briefing_in(tmp.path(), kind, reason)
}

/// [`briefing`] under `root`, so two briefings can share the work dir their
/// instructions name.
fn briefing_in(root: &std::path::Path, kind: DisputeKind, reason: &str) -> Prompt {
    staged_briefing_in(root, &stage(), kind, reason)
}

/// [`briefing_in`] for `stage`.
fn staged_briefing_in(
    root: &std::path::Path,
    stage: &Stage,
    kind: DisputeKind,
    reason: &str,
) -> Prompt {
    let plan = root.join("PLAN.md");
    std::fs::write(&plan, "stub plan").unwrap();
    let work = root.join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    build(&plan, stage, &request(kind, reason), &work)
}

/// A kind briefing is not the criterion briefing, names the disputed `id`,
/// and tells the session how to record its verdict.
fn assert_kind_briefing(prompt: &Prompt, id: &str) {
    assert!(
        !prompt.instructions.contains("RUN THE CRITERIA"),
        "routed to the criterion builder:\n{}",
        prompt.instructions
    );
    assert!(
        prompt.instructions.contains(RECORD_COMMAND),
        "no verdict protocol:\n{}",
        prompt.instructions
    );
    assert!(
        prompt.render().contains(id),
        "the briefing never names {id}:\n{}",
        prompt.render()
    );
}

#[test]
fn criterion_dispute_reaches_the_criterion_builder() {
    let prompt = briefing(
        DisputeKind::criterion(CriterionField::Acceptance, vec![0]),
        "criterion impossible",
    );
    assert!(prompt.instructions.contains("RUN THE CRITERIA"));
    assert!(prompt.instructions.contains(RECORD_COMMAND));
    assert!(prompt.evidence.contains("→ [0] cargo test"));
}

#[test]
fn findings_dispute_reaches_the_findings_builder() {
    assert_kind_briefing(&briefing(findings_kind(), "the finding is wrong"), "F-1-2");
}

#[test]
fn contract_dispute_reaches_the_contract_builder() {
    assert_kind_briefing(
        &briefing(contract_kind(), "the contract is wrong"),
        CONTRACT_ID,
    );
}

#[test]
fn integrity_dispute_reaches_the_integrity_builder() {
    assert_kind_briefing(
        &briefing(integrity_kind(), "tests were merged"),
        "TI-decl-rust",
    );
}

/// The cap binds every kind, and trimming never reaches the instructions.
#[test]
fn every_kind_keeps_the_byte_cap_and_whole_instructions() {
    let long_reason = "the reason runs on. ".repeat(10_000);
    for kind in [findings_kind(), contract_kind(), integrity_kind()] {
        let tmp = tempfile::tempdir().unwrap();
        let short = briefing_in(tmp.path(), kind.clone(), "short");
        let long = briefing_in(tmp.path(), kind, &long_reason);
        assert!(
            long.total_len() <= MAX_PROMPT_BYTES,
            "briefing {} exceeded {MAX_PROMPT_BYTES}",
            long.total_len()
        );
        assert_eq!(
            long.instructions, short.instructions,
            "the reason belongs in the evidence, and the instructions are never trimmed"
        );
    }
}

fn wiring_check(source: &str, pattern: &str) -> WiringCheck {
    WiringCheck {
        source: source.to_string(),
        pattern: pattern.to_string(),
        description: format!("{pattern} is wired"),
        literal: false,
    }
}

/// A wiring dispute names the disputed check, searches for it the way plan v2
/// does (its `source` is a glob), and lets the verdict amend only `wiring`.
#[test]
fn a_wiring_dispute_shows_the_wiring_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let stage = Stage {
        wiring: vec![
            wiring_check("src/main.rs", "run_first"),
            wiring_check("src/cli/*.rs", r"dispatch\("),
        ],
        ..stage()
    };
    let kind = DisputeKind::criterion(CriterionField::Wiring, vec![1]);
    let prompt = staged_briefing_in(tmp.path(), &stage, kind, "the pattern is wrong");

    let instructions = &prompt.instructions;
    assert!(
        instructions.contains("RUN THE WIRING CHECK"),
        "{instructions}"
    );
    assert!(
        instructions.contains(r"- pattern: `dispatch\(`"),
        "{instructions}"
    );
    assert!(
        instructions.contains("- source: `src/cli/*.rs`"),
        "{instructions}"
    );
    let search = r"rg -n -e 'dispatch\(' --no-ignore --hidden --glob 'src/cli/*.rs' .";
    assert!(instructions.contains(search), "{instructions}");
    assert!(
        instructions.contains("\"field\": \"wiring\""),
        "{instructions}"
    );
    assert!(
        !instructions.contains("\"acceptance\" | \"wiring\""),
        "{instructions}"
    );
    assert!(instructions.contains(RECORD_COMMAND), "{instructions}");
    assert!(
        prompt
            .evidence
            .contains(r"→ [1] `dispatch\(` in `src/cli/*.rs`"),
        "{}",
        prompt.evidence
    );
    assert!(prompt
        .evidence
        .contains("  [0] `run_first` in `src/main.rs`"));
}

/// A wiring-tests dispute runs the test's own command from the execution site
/// and lets the verdict amend only `wiring-tests`.
#[test]
fn a_wiring_tests_dispute_runs_the_wiring_test_command() {
    let tmp = tempfile::tempdir().unwrap();
    let stage = Stage {
        wiring_tests: vec![WiringTest {
            name: "help lists dispute-criteria".to_string(),
            command: "loom stage --help".to_string(),
            success_criteria: SuccessCriteria {
                stdout_contains: vec!["dispute-criteria".to_string()],
                ..SuccessCriteria::default()
            },
            description: None,
        }],
        ..stage()
    };
    let kind = DisputeKind::criterion(CriterionField::WiringTests, vec![0]);
    let prompt = staged_briefing_in(tmp.path(), &stage, kind, "the wiring test is wrong");

    let instructions = &prompt.instructions;
    assert!(
        instructions.contains("RUN THE WIRING TEST"),
        "{instructions}"
    );
    assert!(
        instructions.contains("loom stage --help\necho \"exit: $?\"\n"),
        "{instructions}"
    );
    assert!(
        instructions.contains(r#""stdout_contains":["dispute-criteria"]"#),
        "{instructions}"
    );
    assert!(
        instructions.contains("\"field\": \"wiring-tests\""),
        "{instructions}"
    );
    assert!(
        prompt
            .evidence
            .contains("→ [0] help lists dispute-criteria: `loom stage --help`"),
        "{}",
        prompt.evidence
    );
}

/// An index past the list says the entry may have been amended away.
#[test]
fn a_wiring_dispute_past_the_list_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let kind = DisputeKind::criterion(CriterionField::Wiring, vec![3]);
    let prompt = staged_briefing_in(tmp.path(), &stage(), kind, "the pattern is wrong");
    assert!(
        prompt
            .instructions
            .contains("no longer has a wiring check at index 3"),
        "{}",
        prompt.instructions
    );
}
