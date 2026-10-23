//! Round trips for the dispute kinds beyond a criterion (DESIGN D15), through
//! the same stand-in for the adjudication session as the criterion tests,
//! and for a refused contract freeze against a real stage worktree.
//! Declared from `adjudication_e2e.rs`; `use super::*;` reaches its fixtures.

use super::*;
use loom::models::dispute::{DisputeKind, DisputeVerdict, DisputeVerdictRecord, FindingSnapshot};
use loom::verify::contracts::store::load_freeze;
use loom::verify::review::report::Finding;
use loom::verify::review::store::{self, RecordedFinding, ReviewRound, RulingKind};

fn finding() -> Finding {
    Finding {
        severity: "major".to_string(),
        file: "src/a.rs".to_string(),
        line: 4,
        claim: "leaks the handle".to_string(),
        scenario: Some("early return → handle never closed".to_string()),
        rule: None,
    }
}

/// One harvested review round for `stage_id` holding finding `F-1-1`.
fn record_review(work: &Path, stage_id: &str) {
    let round = ReviewRound {
        version: store::RECORD_VERSION,
        round: 1,
        agent_id: "reviewer".to_string(),
        harvested_at: chrono::Utc::now(),
        fingerprint: "sha256:0".to_string(),
        files: std::collections::BTreeMap::new(),
        malformed: None,
        findings: vec![RecordedFinding {
            id: "F-1-1".to_string(),
            finding: finding(),
        }],
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    };
    store::write_round(work, stage_id, &round).unwrap();
}

/// Dispute 1 of `stage_id` against `F-1-1`, as the daemon files it.
fn write_findings_dispute(work: &Path, stage_id: &str) {
    let disputes_root = work.join("disputes");
    std::fs::create_dir_all(disputes_root.join(stage_id).join("1")).unwrap();
    let evidence = FindingSnapshot {
        id: "F-1-1".to_string(),
        origin_stage: None,
        round: 1,
        finding: finding(),
    };
    let request = DisputeRequest {
        id: 1,
        stage_id: stage_id.to_string(),
        kind: DisputeKind::Findings {
            finding_ids: vec![evidence.id.clone()],
            evidence: vec![evidence],
        },
        reason: "the early return never runs with a handle open".to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: chrono::Utc::now(),
    };
    let yaml = loom::yaml::to_string(&request).unwrap();
    let path = request_file(&disputes_root, stage_id, 1);
    std::fs::write(path, format!("---\n{yaml}---\n\n# Dispute\n")).unwrap();
}

#[test]
fn findings_dispute_round_trip() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path();
    write_stage(work, &make_stage("s1"));
    record_review(work, "s1");
    write_findings_dispute(work, "s1");
    let verdict = serde_json::json!({
        "verdict": "rulings",
        "rulings": [{
            "finding": "F-1-1",
            "ruling": "dismiss",
            "reasoning": "the handle is closed by its guard on every return path",
            "citations": [
                {"file": "src/a.rs", "line": 4, "excerpt": "let _guard", "claim": "guard closes it"}
            ]
        }]
    });

    drive_dispute(&AdjudicatorRegistry::new(), work, "s1", 1, &verdict);

    let rulings = store::load_rulings(work, "s1").unwrap().rulings;
    assert_eq!(rulings.len(), 1);
    assert_eq!(rulings[0].finding, "F-1-1");
    assert_eq!(rulings[0].ruling, RulingKind::Dismiss);
    assert_eq!(rulings[0].dispute, 1);
    assert!(store::open_findings(work, "s1").unwrap().is_empty());
    let after = loom::verify::transitions::load_stage("s1", work).unwrap();
    assert_eq!(after.status, StageStatus::Queued);
    assert!(applied_marker(&work.join("disputes"), "s1", 1).exists());
}

/// A v2 plan whose stage `s1` holds one Rust contract, one harness glob and
/// the format gate `cargo fmt --check`.
const REFUSAL_PLAN: &str = r#"# Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  stages:
    - id: s1
      name: s1
      stage_type: standard
      working_dir: "."
      harness: ["tests/support/**"]
      contracts:
        - id: rejects-x
          file: "tests/x_contract.rs"
          test: "rejects_x"
          runner: cargo-test
          scenario: "x arrives"
          rejects: "accepting x"
      acceptance:
        - "cargo test"
        - "cargo fmt --check"
```

<!-- END loom METADATA -->
"#;

/// A refused freeze under adjudication: a real repository whose base holds
/// a Cargo package (`src/lib.rs`, `src/main.rs`), the stage worktree on
/// `loom/s1` holding the untracked contract file, the plan named in the
/// state directory's config, and dispute 1 over the refusal.
struct Refused {
    _tmp: tempfile::TempDir,
    work: PathBuf,
    plan: PathBuf,
}

fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

fn refused_freeze() -> Refused {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    let manifest = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
    std::fs::write(repo.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "").unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {}\n").unwrap();
    git(&repo, &["add", "Cargo.toml", "src/lib.rs", "src/main.rs"]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    git(
        &repo,
        &["worktree", "add", "-q", "-b", "loom/s1", ".worktrees/s1"],
    );
    let tests = repo.join(".worktrees/s1/tests");
    std::fs::create_dir_all(&tests).unwrap();
    std::fs::write(
        tests.join("x_contract.rs"),
        "#[test]\nfn rejects_x() { panic!() }\n",
    )
    .unwrap();
    let workspace = loom::fs::work_dir::WorkDir::new(&repo).unwrap();
    workspace.initialize().unwrap();
    let work = workspace.root().to_path_buf();
    let plan = repo.join("doc/plans/PLAN-refusal.md");
    std::fs::create_dir_all(plan.parent().unwrap()).unwrap();
    std::fs::write(&plan, REFUSAL_PLAN).unwrap();
    write_config(&work, &plan);
    write_stage(&work, &refused_stage());
    write_refusal_dispute(&work);
    Refused {
        _tmp: tmp,
        work,
        plan,
    }
}

/// `s1` as the plan states it, under adjudication of its refused freeze.
fn refused_stage() -> Stage {
    Stage {
        plan_version: 2,
        worktree: Some("s1".to_string()),
        working_dir: Some(".".to_string()),
        harness: vec!["tests/support/**".to_string()],
        contracts: vec![loom::plan::schema::ContractSpec {
            id: "rejects-x".to_string(),
            file: "tests/x_contract.rs".to_string(),
            test: "rejects_x".to_string(),
            runner: Some("cargo-test".to_string()),
            cwd: None,
            scenario: "x arrives".to_string(),
            rejects: "accepting x".to_string(),
        }],
        acceptance: vec![
            AcceptanceCriterion::Simple("cargo test".to_string()),
            AcceptanceCriterion::Simple("cargo fmt --check".to_string()),
        ],
        ..make_stage("s1")
    }
}

/// Dispute 1 as loom files it for a refused freeze.
fn write_refusal_dispute(work: &Path) {
    let disputes_root = work.join("disputes");
    std::fs::create_dir_all(disputes_root.join("s1").join("1")).unwrap();
    let request = DisputeRequest {
        id: 1,
        stage_id: "s1".to_string(),
        kind: DisputeKind::FreezeRefusal {
            problems: vec!["writer-reported: tests/support cannot hold the fixture".to_string()],
        },
        reason: "contract freeze refused: writer-reported: tests/support cannot hold the fixture"
            .to_string(),
        evidence_commit: None,
        failure_output: None,
        fix_attempts_at_dispute: 0,
        created_at: chrono::Utc::now(),
    };
    let yaml = loom::yaml::to_string(&request).unwrap();
    let path = request_file(&disputes_root, "s1", 1);
    std::fs::write(path, format!("---\n{yaml}---\n\n# Dispute\n")).unwrap();
}

fn refusal_citations() -> serde_json::Value {
    serde_json::json!([{"file": "tests/x_contract.rs", "line": 1, "excerpt": "fn rejects_x()",
                        "claim": "the contract reads a fixture outside the harness"}])
}

/// Record `verdict` as the judge does, reload what was recorded, then apply it.
fn record_reload_apply(fx: &Refused, verdict: &serde_json::Value) -> DisputeVerdict {
    assert_eq!(
        session_records_verdict(&fx.work, "s1", 1, verdict),
        AdjudicateOutcome::Recorded
    );
    let path = verdict_file(&fx.work.join("disputes"), "s1", 1);
    let content = std::fs::read_to_string(path).unwrap();
    let record: DisputeVerdictRecord =
        loom::yaml::from_str(content.split("---").nth(1).unwrap()).unwrap();
    AdjudicatorRegistry::new()
        .apply_pending_verdicts(&fx.work)
        .unwrap();
    assert!(applied_marker(&fx.work.join("disputes"), "s1", 1).exists());
    record.verdict
}

/// The conditions under which the next spawn is a contract writer on the
/// same worktree: queued, v2 with contracts, nothing frozen.
fn assert_queued_for_a_contract_writer(fx: &Refused) -> Stage {
    let stage = loom::verify::transitions::load_stage("s1", &fx.work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert_eq!(stage.plan_version, 2);
    assert!(!stage.contracts.is_empty());
    assert!(load_freeze(&fx.work, "s1").unwrap().is_none());
    stage
}

#[test]
fn freeze_refusal_accept_with_a_harness_patch_requeues_for_a_contract_writer() {
    let fx = refused_freeze();
    let verdict = serde_json::json!({
        "verdict": "accept",
        "reasoning": "the contract needs a fixture the harness does not cover",
        "citations": refusal_citations(),
        "plan_patch": {"patches": [
            {"field": "harness", "op": "insert", "index": 0, "value": "tests/fixtures/**"}
        ]}
    });

    let recorded = record_reload_apply(&fx, &verdict);

    assert!(
        matches!(recorded, DisputeVerdict::Accept { .. }),
        "{recorded:?}"
    );
    let stage = assert_queued_for_a_contract_writer(&fx);
    assert_eq!(stage.harness, ["tests/fixtures/**", "tests/support/**"]);
    assert!(feedback::read_feedback(&fx.work, "s1").unwrap().is_some());
}

#[test]
fn freeze_refusal_accept_applies_two_patches_in_order() {
    let fx = refused_freeze();
    let verdict = serde_json::json!({
        "verdict": "accept",
        "reasoning": "the fixture needs a harness glob and the gate must cover the workspace",
        "citations": refusal_citations(),
        "plan_patch": {"patches": [
            {"field": "harness", "op": "insert", "index": 0, "value": "tests/fixtures/**"},
            {"field": "acceptance", "op": "replace", "index": 1, "value": "cargo fmt --all --check"}
        ]}
    });

    record_reload_apply(&fx, &verdict);

    let stage = assert_queued_for_a_contract_writer(&fx);
    assert_eq!(stage.harness[0], "tests/fixtures/**");
    assert_eq!(
        stage.acceptance[1],
        AcceptanceCriterion::Simple("cargo fmt --all --check".to_string())
    );
    let plan = std::fs::read_to_string(&fx.plan).unwrap();
    assert!(plan.contains("tests/fixtures/**") && plan.contains("cargo fmt --all --check"));
    let notice = feedback::read_feedback(&fx.work, "s1").unwrap().unwrap();
    let harness = notice
        .find("harness: inserted at 0")
        .expect("the harness amendment");
    let gate = notice
        .find("acceptance: replaced 1")
        .expect("the gate amendment");
    assert!(harness < gate, "{notice}");
}

#[test]
fn freeze_refusal_reject_hands_its_instructions_to_the_next_writer() {
    let fx = refused_freeze();
    let reasoning = "the writer's fixture sits outside tests/support, which the harness covers";
    let instructions = "move the fixture under tests/support/ and freeze again";
    let verdict = serde_json::json!({
        "verdict": "reject",
        "reasoning": reasoning,
        "citations": refusal_citations(),
        "instructions": instructions
    });

    let recorded = record_reload_apply(&fx, &verdict);

    let DisputeVerdict::Reject {
        reasoning: stored, ..
    } = recorded
    else {
        panic!("expected a reject, got {recorded:?}");
    };
    assert_eq!(
        stored,
        format!("{reasoning}\n\n### Instructions\n\n{instructions}")
    );
    assert_queued_for_a_contract_writer(&fx);
    let notice = feedback::read_feedback(&fx.work, "s1").unwrap().unwrap();
    assert!(notice.contains(instructions), "{notice}");
}

#[test]
fn freeze_refusal_harness_patch_over_production_source_is_refused_at_recording() {
    let fx = refused_freeze();
    let verdict = serde_json::json!({
        "verdict": "accept",
        "reasoning": "the writer needs to edit main",
        "citations": refusal_citations(),
        "plan_patch": {"patches": [
            {"field": "harness", "op": "insert", "index": 0, "value": "src/main.rs"}
        ]}
    });
    let draft = verdict_draft_file(&fx.work, "s1", 1);
    std::fs::create_dir_all(draft.parent().unwrap()).unwrap();
    std::fs::write(&draft, verdict.to_string()).unwrap();

    let error = record_verdict(&fx.work, "s1", 1, &draft, None).unwrap_err();

    assert!(
        format!("{error:#}").contains("production source"),
        "{error:#}"
    );
    assert!(!verdict_file(&fx.work.join("disputes"), "s1", 1).exists());
    let stage = loom::verify::transitions::load_stage("s1", &fx.work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsAdjudication);
}
