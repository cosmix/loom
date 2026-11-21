//! The impact-test briefing: what it tells the adjudicator and the evidence
//! it carries.

use super::*;
use crate::models::dispute::DisputeRequest;
use crate::models::stage::Stage;
use crate::orchestrator::adjudication::prompt::ExecutionSite;
use crate::testrun::TestTarget;
use crate::verify::impact_tests::{selection_digest, FailureKind, ImpactFailure, ReplayPlan};
use chrono::Utc;
use std::path::Path;

fn lib_target(name: &str) -> TestTarget {
    TestTarget {
        file: "src/lib.rs".to_string(),
        name: Some(name.to_string()),
    }
}

fn request() -> DisputeRequest {
    let failure = ImpactFailure {
        failed_tests: Vec::new(),
        id: "IT-cargo-test:loom".to_string(),
        command: "cargo test -- verify::x::passes".to_string(),
        tests: vec!["verify::x::passes".to_string()],
        selection_digest: selection_digest(&[lib_target("verify::x::passes")]),
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
    "Command (reported display text, cut when long): `cargo test -- verify::x::passes`",
    "Replay command: none (stage worktree is gone)",
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
        "re-run each disputed selection yourself with its\nreplay command below",
        "return needs-more-evidence asking for a fresh completion run",
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

/// `count` cargo tests failing as one selection: the failure a run records
/// (64 names listed, the digest of all) and the targets it selected.
fn large_selection(count: usize) -> (ImpactFailure, Vec<TestTarget>) {
    let targets: Vec<TestTarget> = (0..count)
        .map(|n| lib_target(&format!("suite::case_{n:03}")))
        .collect();
    let failure = ImpactFailure {
        failed_tests: Vec::new(),
        id: "IT-cargo-test:.".to_string(),
        command: format!("cargo test -- suite::case_000 … ({count} targets)"),
        tests: targets
            .iter()
            .take(64)
            .filter_map(|target| target.name.clone())
            .collect(),
        selection_digest: selection_digest(&targets),
        kind: FailureKind::Fails { failed: Some(1) },
        base: "it passes at the stage's base 0123abcd".to_string(),
        accepted: None,
        output: "1 failed".to_string(),
    };
    (failure, targets)
}

fn failure_section(failure: &ImpactFailure, plan: &Result<ReplayPlan, String>) -> String {
    let mut section = String::new();
    push_failure(&mut section, failure, plan);
    section
}

/// The replay command of a 721-test selection is rebuilt in full from the
/// checkout, not taken from the reported display excerpt.
#[test]
fn a_721_test_selection_is_replayed_in_full() {
    let tmp = tempfile::tempdir().unwrap();
    let (failure, targets) = large_selection(721);
    let plan = Ok(ReplayPlan::of_selection(
        tmp.path(),
        "cargo-test",
        Path::new(""),
        targets,
    ));

    let section = failure_section(&failure, &plan);

    assert!(
        section.contains("(721 targets)"),
        "the display excerpt stays"
    );
    let replay = &section[section.find("Replay command (derived by loom").unwrap()..];
    for n in [0, 63, 64, 720] {
        assert!(replay.contains(&format!("suite::case_{n:03}")), "case {n}");
    }
    assert!(!replay.contains("targets)"), "{replay}");
    assert!(replay.contains(&format!("Run it from `{}`", tmp.path().display())));
}

/// A selection of another runner or package is not the reported one, and a
/// checkout selecting other tests, or evidence without a digest, is stale.
#[test]
fn a_replay_is_refused_unless_the_checkout_selects_the_reported_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let (failure, targets) = large_selection(721);
    let plan_of = |targets: Vec<TestTarget>| {
        Ok(ReplayPlan::of_selection(
            tmp.path(),
            "cargo-test",
            Path::new(""),
            targets,
        ))
    };
    let mut renamed = targets.clone();
    renamed[700] = lib_target("suite::renamed");
    let no_digest = ImpactFailure {
        selection_digest: String::new(),
        ..failure.clone()
    };
    let other_package = ImpactFailure {
        id: "IT-cargo-test:web".to_string(),
        ..failure.clone()
    };
    let other_runner = ImpactFailure {
        id: "IT-vitest:.".to_string(),
        ..failure.clone()
    };

    let cases = [
        (&failure, plan_of(renamed), "selects other tests"),
        (&no_digest, plan_of(targets.clone()), "no selection digest"),
        (
            &other_package,
            plan_of(targets.clone()),
            "no cargo-test tests in `web`",
        ),
        (&other_runner, plan_of(targets), "no vitest tests in `.`"),
        (&failure, Err("a graph error".to_string()), "a graph error"),
    ];
    for (failure, plan, why) in cases {
        let section = failure_section(failure, &plan);
        assert!(section.contains("Replay command: none ("), "{why}");
        assert!(section.contains(why), "{why}: {section}");
        assert!(section.contains("needs-more-evidence"), "{why}");
        assert!(!section.contains("Replay command (derived"), "{why}");
    }
}
