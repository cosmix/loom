//! A freeze-refusal dispute's verdicts applied (`apply_freeze_refusal.rs`):
//! each accept amends the plan and the stage before the stage is re-queued for
//! a fresh contract writer, a reject and a needs-more-evidence re-queue it
//! with their notice, and the amendment cap escalates instead.

use serde_json::json;
use std::path::PathBuf;
use tempfile::TempDir;

use super::super::feedback::read_feedback;
use super::super::tests::{make_stage, write_request, write_stage, write_verdict};
use super::super::AdjudicatorRegistry;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{Citation, DisputeKind, DisputeVerdict, PlanPatch};
use crate::models::stage::{AcceptanceCriterion, Stage, StageStatus};
use crate::plan::amendment::{apply_amendment, AmendmentField, AmendmentPatch, AmendmentRequest};
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::store::load_freeze;
use crate::verify::contracts::test_support::{contract, contract_worktree};
#[cfg(feature = "source-graph")]
use crate::verify::contracts::test_support::{liquidmon_contract, LIQUIDMON_FILE};
use crate::verify::transitions::load_stage;

/// A v2 plan whose stage `s1` holds one contract, one harness glob and a
/// format-check criterion.
const PLAN: &str = r#"# Plan

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
          test: "tests::rejects_x"
          runner: cargo-test
          scenario: "x arrives"
          rejects: "accepting x"
      acceptance:
        - "cargo test"
        - "cargo fmt --check"
```

<!-- END loom METADATA -->
"#;

struct Fixture {
    _tmp: TempDir,
    work: PathBuf,
    plan: PathBuf,
}

/// The stage under adjudication of a refused freeze, its worktree holding the
/// unfrozen contract file, and `plan` named in the state directory's config.
fn fixture(plan: &str) -> Fixture {
    fixture_with(plan, vec![contract()])
}

/// [`fixture`] for a stage holding `contracts`, which `plan` must list too.
fn fixture_with(plan: &str, contracts: Vec<ContractSpec>) -> Fixture {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    contract_worktree(&repo, "s1");
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let work = workspace.root().to_path_buf();
    let plans = repo.join("doc").join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    let plan_path = plans.join("PLAN-refusal.md");
    std::fs::write(&plan_path, plan).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\n\
         base_branch = \"main\"\n",
        plan_path.display()
    );
    std::fs::write(work.join("config.toml"), config).unwrap();
    write_stage(
        &work,
        &Stage {
            plan_version: 2,
            worktree: Some("s1".to_string()),
            working_dir: Some(".".to_string()),
            harness: vec!["tests/support/**".to_string()],
            contracts,
            acceptance: vec![
                AcceptanceCriterion::Simple("cargo test".to_string()),
                AcceptanceCriterion::Simple("cargo fmt --check".to_string()),
            ],
            ..make_stage("s1")
        },
    );
    Fixture {
        _tmp: tmp,
        work,
        plan: plan_path,
    }
}

fn citations() -> Vec<Citation> {
    vec![Citation {
        file: "tests/x_contract.rs".to_string(),
        line: Some(1),
        excerpt: "fn rejects_x()".to_string(),
        claim: "the refusal names it".to_string(),
    }]
}

/// Dispute 1 over a refused freeze, answered by `verdict` and applied.
fn apply(fx: &Fixture, verdict: DisputeVerdict) {
    let kind = DisputeKind::FreezeRefusal {
        problems: vec!["tests/support/data.json is outside the harness".to_string()],
    };
    write_request(&fx.work, "s1", 1, kind);
    write_verdict(&fx.work, "s1", 1, verdict, 1);
    AdjudicatorRegistry::new()
        .apply_verdict(&fx.work, "s1", 1)
        .unwrap();
}

fn accept(fx: &Fixture, patches: serde_json::Value) {
    apply(
        fx,
        DisputeVerdict::Accept {
            plan_patch: PlanPatch {
                inner: json!({ "patches": patches }),
            },
            citations: citations(),
            reasoning: "the plan's harness misses the fixtures".to_string(),
        },
    );
}

fn notice(fx: &Fixture) -> String {
    read_feedback(&fx.work, "s1")
        .unwrap()
        .expect("a notice for the next writer")
}

fn plan_text(fx: &Fixture) -> String {
    std::fs::read_to_string(&fx.plan).unwrap()
}

/// Requeued for a fresh contract writer: queued, still no freeze.
fn assert_requeued_for_a_writer(fx: &Fixture) -> Stage {
    let stage = load_stage("s1", &fx.work).unwrap();
    assert_eq!(stage.status, StageStatus::Queued);
    assert!(load_freeze(&fx.work, "s1").unwrap().is_none());
    stage
}

#[test]
fn an_accepted_harness_patch_amends_plan_and_stage_and_requeues() {
    let fx = fixture(PLAN);

    accept(
        &fx,
        json!([{"field": "harness", "op": "insert", "index": 0, "value": "tests/fixtures/**"}]),
    );

    let stage = assert_requeued_for_a_writer(&fx);
    assert_eq!(stage.harness, ["tests/fixtures/**", "tests/support/**"]);
    assert!(plan_text(&fx).contains("tests/fixtures/**"));
    let notice = notice(&fx);
    assert!(
        notice.contains("harness: inserted at 0: `tests/fixtures/**`"),
        "{notice}"
    );
    assert!(
        notice.contains("loom stage contracts freeze s1"),
        "{notice}"
    );
}

#[test]
fn an_accepted_contracts_patch_names_the_old_file_to_delete() {
    let fx = fixture(PLAN);
    let spec = "id: rejects-x\nfile: tests/y_contract.rs\ntest: tests::rejects_x\n\
                runner: cargo-test\nscenario: x arrives\nrejects: accepting x\n";

    accept(
        &fx,
        json!([{"field": "contracts", "op": "replace", "index": 0, "value": spec}]),
    );

    let stage = assert_requeued_for_a_writer(&fx);
    assert_eq!(stage.contracts[0].file, "tests/y_contract.rs");
    assert!(plan_text(&fx).contains("tests/y_contract.rs"));
    let notice = notice(&fx);
    assert!(
        notice.contains("contracts: replaced 0: contract `rejects-x`"),
        "{notice}"
    );
    assert!(notice.contains("### Old contract files"), "{notice}");
    assert!(notice.contains("`tests/x_contract.rs`"), "{notice}");
}

/// The old contract sat in a module the base does not declare: the notice
/// also lists the scaffold only that contract needed.
#[cfg(feature = "source-graph")]
#[test]
fn an_accepted_contracts_patch_names_the_old_scaffold_to_delete() {
    let plan = PLAN
        .replace("tests/x_contract.rs", LIQUIDMON_FILE)
        .replace("tests::rejects_x", &liquidmon_contract().test);
    let fx = fixture_with(&plan, vec![liquidmon_contract()]);
    let spec = "id: rejects-x\nfile: tests/y_contract.rs\ntest: tests::rejects_x\n\
                runner: cargo-test\nscenario: x arrives\nrejects: accepting x\n";

    accept(
        &fx,
        json!([{"field": "contracts", "op": "replace", "index": 0, "value": spec}]),
    );

    assert_requeued_for_a_writer(&fx);
    let notice = notice(&fx);
    assert!(
        notice.contains("`src/device_lock/contract_tests.rs`"),
        "{notice}"
    );
    assert!(notice.contains("Their scaffold:"), "{notice}");
    assert!(
        notice.contains("- in `src/lib.rs`, `mod device_lock;`"),
        "{notice}"
    );
    assert!(
        notice.contains("a new `src/device_lock/mod.rs`"),
        "{notice}"
    );
    assert!(
        notice.contains("`#[cfg(test)] mod contract_tests;`"),
        "{notice}"
    );
}

#[test]
fn an_unreadable_old_scaffold_is_said_to_be_unknown() {
    let fx = fixture(PLAN);
    let mut stage = load_stage("s1", &fx.work).unwrap();
    let before = stage.contracts.clone();
    stage.contracts[0].file = "tests/y_contract.rs".to_string();
    stage.worktree = Some("gone".to_string());

    let section = super::old_files_section(&fx.work, &stage, &before);

    assert!(section.contains("- `tests/x_contract.rs`"), "{section}");
    assert!(
        section.contains("(their scaffold is unknown: "),
        "{section}"
    );
    assert!(!section.contains("Their scaffold:"), "{section}");
}

#[test]
fn an_accepted_acceptance_replace_swaps_the_format_check() {
    let fx = fixture(PLAN);

    accept(
        &fx,
        json!([{"field": "acceptance", "op": "replace", "index": 1, "value": "cargo fmt --all -- --check"}]),
    );

    let stage = assert_requeued_for_a_writer(&fx);
    assert_eq!(
        stage.acceptance[1],
        AcceptanceCriterion::Simple("cargo fmt --all -- --check".to_string())
    );
    assert!(notice(&fx).contains("acceptance: replaced 1"));
}

#[test]
fn a_reject_requeues_with_the_instructions_in_its_notice() {
    let fx = fixture(PLAN);
    let reasoning = "src/lib.rs is production source the writer changed\n\n### Instructions\n\n\
                     revert src/lib.rs, then freeze";

    apply(
        &fx,
        DisputeVerdict::Reject {
            citations: citations(),
            reasoning: reasoning.to_string(),
        },
    );

    assert_requeued_for_a_writer(&fx);
    let notice = notice(&fx);
    assert!(
        notice.contains("rejected the refused contract freeze"),
        "{notice}"
    );
    assert!(
        notice.contains("### Instructions\n\nrevert src/lib.rs, then freeze"),
        "{notice}"
    );
    assert!(
        notice.contains("loom stage contracts freeze s1"),
        "{notice}"
    );
}

#[test]
fn needs_more_evidence_asks_for_memory_notes_and_requeues() {
    let fx = fixture(PLAN);

    apply(
        &fx,
        DisputeVerdict::NeedsMoreEvidence {
            questions: vec!["Which fixture does\nrejects-x read?".to_string()],
        },
    );

    let stage = assert_requeued_for_a_writer(&fx);
    assert_eq!(stage.tally.evidence_rounds, 1);
    let notice = notice(&fx);
    assert!(
        notice.contains("loom memory note \"freeze-dispute: answer <n>: <evidence>\""),
        "{notice}"
    );
    assert!(
        notice.contains("loom stage contracts freeze s1"),
        "{notice}"
    );
    assert!(
        notice.contains("1. Which fixture does rejects-x read?"),
        "{notice}"
    );
}

#[test]
fn the_amendment_cap_escalates_and_does_not_requeue() {
    let capped = PLAN.replace(
        "  stages:\n",
        "  adjudication:\n    max_amendments_per_stage: 1\n  stages:\n",
    );
    let fx = fixture(&capped);
    spend_the_cap(&fx);
    let before = plan_text(&fx);

    accept(
        &fx,
        json!([{"field": "harness", "op": "insert", "index": 0, "value": "tests/fixtures/**"}]),
    );

    assert_eq!(plan_text(&fx), before);
    let stage = load_stage("s1", &fx.work).unwrap();
    assert_eq!(stage.status, StageStatus::NeedsHumanReview);
    assert!(read_feedback(&fx.work, "s1").unwrap().is_none());
}

/// One amendment from an earlier verdict, which a cap of one allows.
fn spend_the_cap(fx: &Fixture) {
    let earlier = AmendmentRequest {
        stage_id: "s1".to_string(),
        field: AmendmentField::Harness,
        patch: AmendmentPatch::Insert {
            index: 1,
            value: "tests/data/**".to_string(),
        },
        reason: Some("an earlier verdict".to_string()),
        dispute_id: Some("0".to_string()),
    };
    apply_amendment(&fx.plan, &fx.work, earlier).unwrap();
}

#[test]
fn the_old_file_section_is_empty_while_every_contract_file_stands() {
    let fx = fixture(PLAN);
    let stage = load_stage("s1", &fx.work).unwrap();
    assert!(super::old_files_section(&fx.work, &stage, &stage.contracts).is_empty());
}
