//! The impact-test briefing: what it tells the adjudicator and the evidence
//! it carries.

use super::*;
use crate::models::dispute::DisputeRequest;
use crate::models::stage::Stage;
use crate::orchestrator::adjudication::prompt::ExecutionSite;
use crate::verify::impact_tests::{FailureKind, ImpactFailure};
use chrono::Utc;

fn request() -> DisputeRequest {
    let failure = ImpactFailure {
        id: "IT-cargo-test:loom".to_string(),
        command: "cargo test -- verify::x::passes".to_string(),
        tests: vec!["verify::x::passes".to_string()],
        kind: FailureKind::Fails { failed: Some(1) },
        base: "the stage's base 0123abcd proved nothing: `cargo test` timed out after 300 s"
            .to_string(),
        accepted: Some("worse than dispute 2 accepted (fails)".to_string()),
        output: "thread 'verify::x::passes' panicked at src/verify/x.rs:9".to_string(),
    };
    DisputeRequest {
        id: 5,
        stage_id: "demo".to_string(),
        kind: DisputeKind::ImpactTests {
            test_ids: vec![failure.id.clone(), "IT-vitest:web".to_string()],
            evidence: vec![failure],
        },
        reason: "the panic is in a module this stage never touched".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: Utc::now(),
    }
}

/// What the evidence of [`request`] shows, each source labelled: the stage
/// session's unverified report, the agent's reason.
const EVIDENCE: &[&str] = &[
    "Stage base (read by loom now): unavailable: stage worktree is gone",
    "## Failing selections (reported by the stage session, unverified)",
    "the filing agent's request added nothing to it",
    "### IT-cargo-test:loom",
    "Command (reported): `cargo test -- verify::x::passes`",
    "Outcome (reported): fails (1 failed)",
    "At the base (reported): the stage's base 0123abcd proved nothing",
    "Earlier dispute (reported): worse than dispute 2 accepted (fails)",
    "Selected tests (reported): verify::x::passes",
    "Output, its end (reported)",
    "panicked at src/verify/x.rs:9",
    "### IT-vitest:web\n\n(snapshot unavailable)",
    "## Agent's reason (written by the stage agent, unverified)",
    "the panic is in a module this stage never touched",
    "stage diff unavailable: stage worktree is gone",
];

#[test]
fn the_briefing_carries_each_failure_its_base_and_the_reason() {
    let tmp = tempfile::tempdir().unwrap();
    let stage = Stage {
        id: "demo".to_string(),
        ..Stage::default()
    };
    let request = request();
    let site = ExecutionSite {
        path: tmp.path().to_path_buf(),
        working_dir: ".".to_string(),
        worktree_present: false,
        root: tmp.path().to_path_buf(),
    };
    let input = KindPromptInput {
        stage: &stage,
        dispute_id: 5,
        request: &request,
        site: &site,
        worktree: None,
        work_dir: tmp.path(),
    };

    let prompt = build(&input);

    for expected in EVIDENCE {
        assert!(prompt.evidence.contains(expected), "missing {expected}");
    }
    assert!(prompt
        .instructions
        .contains("ONE disputed impact-test request"));
    assert!(prompt
        .instructions
        .contains("loom stage adjudicate --stage demo --dispute 5"));
    assert!(prompt
        .instructions
        .contains("An impact-test accept has no plan_patch."));
    for expected in [
        "Nothing authenticates it",
        "re-run each disputed\nselection's command yourself",
        "check out the stage base below on its own",
        "A failure you cannot reproduce is not accepted.",
    ] {
        assert!(prompt.instructions.contains(expected), "missing {expected}");
    }
    assert!(!prompt.evidence.contains("(loom)"), "{}", prompt.evidence);
}

/// The stage's own changes since its base reach the adjudicator as a diff,
/// with the base commit loom reads, for the adjudicator to check out.
#[test]
fn the_briefing_shows_the_stage_diff_from_its_base() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = crate::verify::contracts::test_support::contract_worktree(&repo, "demo");
    std::fs::write(worktree.join("README.md"), "changed by the stage\n").unwrap();
    let stage = Stage {
        id: "demo".to_string(),
        ..Stage::default()
    };
    let request = request();
    let site = ExecutionSite {
        path: worktree.clone(),
        working_dir: ".".to_string(),
        worktree_present: true,
        root: worktree.clone(),
    };
    let work_dir = repo.join(".loom").join("work");
    let input = KindPromptInput {
        stage: &stage,
        dispute_id: 5,
        request: &request,
        site: &site,
        worktree: Some(&worktree),
        work_dir: &work_dir,
    };

    let evidence = build(&input).evidence;

    let diff = &evidence[evidence.find(STAGE_DIFF_HEADING).unwrap()..];
    assert!(diff.contains("-base\n+changed by the stage"), "{diff}");
    let base_line = evidence
        .lines()
        .find(|line| line.starts_with("Stage base (read by loom now): "))
        .unwrap();
    assert!(!base_line.contains("unavailable"), "{base_line}");
}
