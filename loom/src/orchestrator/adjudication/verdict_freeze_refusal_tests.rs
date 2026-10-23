//! The freeze-refusal rules, checked at recording against a real stage
//! worktree (`verdict_freeze_refusal.rs`): one test per rule.

use super::*;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::orchestrator::adjudication::verdict::{parse_and_validate_in, RecordContext};
use crate::verify::contracts::test_support::{
    contract, contract_worktree, liquidmon_contract, write_files, CONTRACT_CONTENT, LIQUIDMON_FILE,
};
use std::path::{Path, PathBuf};

/// `s1` as [`fixture`]'s stage states it, the plan the apply amends.
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
    _tmp: tempfile::TempDir,
    work: PathBuf,
    worktree: PathBuf,
    stage: Stage,
}

/// A refused stage `s1` with one contract, one harness glob and a
/// format-check criterion, its worktree beside the state directory and its
/// plan named in the state directory's config.
fn fixture() -> Fixture {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let work = workspace.root().to_path_buf();
    let plan = tmp.path().join("PLAN.md");
    std::fs::write(&plan, PLAN).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\n\
         base_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work.join("config.toml"), config).unwrap();
    let stage = Stage {
        id: "s1".to_string(),
        plan_version: 2,
        worktree: Some("s1".to_string()),
        working_dir: Some(".".to_string()),
        harness: vec!["tests/support/**".to_string()],
        contracts: vec![contract()],
        acceptance: vec![
            AcceptanceCriterion::Simple("cargo test".to_string()),
            AcceptanceCriterion::Simple("cargo fmt --check".to_string()),
        ],
        ..Stage::default()
    };
    Fixture {
        _tmp: tmp,
        work,
        worktree,
        stage,
    }
}

fn refusal() -> DisputeKind {
    DisputeKind::FreezeRefusal {
        problems: vec!["src/lib.rs changed".to_string()],
    }
}

fn check(work: &Path, stage: &Stage, verdict: &Value) -> Result<DisputeVerdict, String> {
    let context = RecordContext {
        work_dir: work,
        stage,
    };
    match parse_and_validate_in(&verdict.to_string(), &refusal(), &context)? {
        ValidationOutcome::Verdict(verdict) => Ok(verdict),
        other => panic!("a freeze-refusal verdict never escalates: {other:?}"),
    }
}

fn accepting(patches: Value) -> Value {
    json!({"verdict": "accept", "reasoning": "r", "citations": citation(),
           "plan_patch": {"patches": patches}})
}

/// The defect `verdict` is refused for, which must name `needle`.
fn refused(fx: &Fixture, verdict: Value, needle: &str) {
    let error = check(&fx.work, &fx.stage, &verdict).unwrap_err();
    assert!(error.contains(needle), "expected '{needle}' in: {error}");
}

#[test]
fn a_reject_keeps_its_instructions_under_their_heading() {
    let fx = fixture();
    let raw = json!({"verdict": "reject", "reasoning": "the writer's fault",
                     "citations": citation(), "instructions": "move the fixture"});
    let Ok(DisputeVerdict::Reject { reasoning, .. }) = check(&fx.work, &fx.stage, &raw) else {
        panic!("a reject with instructions records");
    };
    assert_eq!(
        reasoning,
        "the writer's fault\n\n### Instructions\n\nmove the fixture"
    );
}

#[test]
fn a_reject_without_instructions_is_refused() {
    let raw = json!({"verdict": "reject", "reasoning": "r", "citations": citation()});
    refused(&fixture(), raw, "instructions");
}

#[test]
fn an_ungrounded_verdict_is_refused() {
    let fx = fixture();
    let no_reasoning = json!({"verdict": "reject", "citations": citation(), "instructions": "i"});
    refused(&fx, no_reasoning, "reasoning");
    let no_citation = json!({"verdict": "reject", "reasoning": "r", "citations": [],
                             "instructions": "i"});
    refused(&fx, no_citation, "citation");
}

#[test]
fn an_accept_without_patches_or_on_another_field_is_refused() {
    let fx = fixture();
    refused(&fx, accepting(json!([])), "at least one entry");
    let wiring = json!([{"field": "wiring", "op": "delete", "index": 0}]);
    refused(&fx, accepting(wiring), "field 'wiring'");
}

#[test]
fn an_acceptance_entry_only_replaces_a_format_check_with_one() {
    let fx = fixture();
    let delete = json!([{"field": "acceptance", "op": "delete", "index": 1}]);
    refused(&fx, accepting(delete), "only replace");
    let not_format = json!([{"field": "acceptance", "op": "replace", "index": 0,
                             "value": "cargo fmt --check"}]);
    refused(&fx, accepting(not_format), "is not a format check");
    let into_test = json!([{"field": "acceptance", "op": "replace", "index": 1,
                            "value": "cargo test"}]);
    refused(
        &fx,
        accepting(into_test),
        "replacement `cargo test` is not a format check",
    );
}

/// An output expectation changes what the format-check gate accepts: with
/// `exit_code: 1` a formatted tree would fail it and an unformatted one pass.
#[test]
fn an_acceptance_replacement_may_not_expect_any_output() {
    let fx = fixture();
    for expectation in [
        "exit_code: 1",
        "stdout_contains: [\"ok\"]",
        "stdout_not_contains: [\"Diff in\"]",
        "stderr_empty: true",
    ] {
        let value = format!("command: cargo fmt --check\n{expectation}\n");
        let replace = json!([{"field": "acceptance", "op": "replace", "index": 1, "value": value}]);
        refused(&fx, accepting(replace), "sets an output expectation");
    }
}

#[test]
fn an_acceptance_replacement_may_be_plain_or_carry_only_a_timeout() {
    let fx = fixture();
    for value in [
        "cargo fmt --all -- --check",
        "command: cargo fmt --check\ntimeout_secs: 120\n",
        "command: cargo fmt --check\nexit_code: 0\n",
    ] {
        let replace = json!([{"field": "acceptance", "op": "replace", "index": 1, "value": value}]);
        let verdict = check(&fx.work, &fx.stage, &accepting(replace));
        assert!(verdict.is_ok(), "{value}: {verdict:?}");
    }
}

#[test]
fn a_harness_glob_over_production_source_is_refused() {
    let glob = json!([{"field": "harness", "op": "insert", "index": 0, "value": "src/**"}]);
    refused(&fixture(), accepting(glob), "src/lib.rs");
}

/// The liquidmon shape: `src/device_lock/mod.rs` does not exist yet, but
/// the derived scaffold creates it, so a glob over it is production source.
#[test]
fn a_harness_glob_over_a_scaffold_root_not_yet_written_is_refused() {
    let mut fx = fixture();
    std::fs::remove_file(fx.worktree.join("tests/x_contract.rs")).unwrap();
    let content = std::str::from_utf8(CONTRACT_CONTENT).unwrap();
    write_files(&fx.worktree, &[(LIQUIDMON_FILE, content)]);
    fx.stage.contracts = vec![liquidmon_contract()];
    let glob = json!([{"field": "harness", "op": "insert", "index": 0,
                       "value": "src/device_lock/**"}]);
    refused(&fx, accepting(glob), "src/device_lock/mod.rs");
}

#[test]
fn each_index_is_read_against_the_field_as_earlier_entries_left_it() {
    let fx = fixture();
    let replay = json!([
        {"field": "harness", "op": "delete", "index": 0},
        {"field": "harness", "op": "replace", "index": 0, "value": "tests/data/**"},
    ]);
    refused(
        &fx,
        accepting(replay),
        "patches[1]: index 0 is out of range",
    );
}

#[test]
fn an_accept_with_no_locatable_worktree_is_refused() {
    let mut fx = fixture();
    fx.stage.worktree = Some("gone".to_string());
    let glob = json!([{"field": "harness", "op": "insert", "index": 0, "value": "tests/f/**"}]);
    refused(&fx, accepting(glob), "worktree cannot be located");
}

#[test]
fn questions_are_required_and_bad_json_is_refused() {
    let fx = fixture();
    refused(
        &fx,
        json!({"verdict": "needs-more-evidence", "questions": []}),
        "question",
    );
    let context = RecordContext {
        work_dir: &fx.work,
        stage: &fx.stage,
    };
    let error = parse_and_validate_in("not json", &refusal(), &context).unwrap_err();
    assert!(error.contains("not valid JSON"), "{error}");
}

/// A contract spec value, `contract()` with `fields` overridden.
fn contract_value(fields: Value) -> String {
    let mut value = serde_json::to_value(contract()).unwrap();
    for (key, field) in fields.as_object().unwrap() {
        value[key] = field.clone();
    }
    value.to_string()
}

/// The plan schema the apply runs refuses a second contract under one id.
#[test]
fn a_duplicate_contract_id_is_refused() {
    let twin = contract_value(json!({"file": "tests/y_contract.rs", "test": "tests::rejects_y"}));
    let patch = json!([{"field": "contracts", "op": "insert", "index": 1, "value": twin}]);
    refused(&fixture(), accepting(patch), "id 'rejects-x' is used twice");
}

#[test]
fn a_contract_file_outside_the_worktree_is_refused() {
    let escaping = contract_value(json!({"file": "../x.rs"}));
    let patch = json!([{"field": "contracts", "op": "replace", "index": 0, "value": escaping}]);
    refused(
        &fixture(),
        accepting(patch),
        "file '../x.rs' cannot contain a `..` component",
    );
}

#[test]
fn deleting_the_only_contract_is_refused() {
    let patch = json!([{"field": "contracts", "op": "delete", "index": 0}]);
    refused(
        &fixture(),
        accepting(patch),
        "at least one entry in `contracts`",
    );
}

/// The apply validates the plan after each entry: a delete that empties the
/// stage's contracts is refused there, whatever the next entry restores.
#[test]
fn a_delete_before_the_insert_that_restores_the_sole_contract_is_refused() {
    let successor = contract_value(json!({"id": "rejects-y", "file": "tests/y_contract.rs",
                                          "test": "tests::rejects_y"}));
    let patch = json!([
        {"field": "contracts", "op": "delete", "index": 0},
        {"field": "contracts", "op": "insert", "index": 0, "value": successor},
    ]);
    let fx = fixture();
    let error = check(&fx.work, &fx.stage, &accepting(patch)).unwrap_err();
    assert!(
        error.contains("after plan_patch.patches[0] (delete contracts[0])"),
        "{error}"
    );
    assert!(
        error.contains("at least one entry in `contracts`"),
        "{error}"
    );
    assert!(error.contains("one `replace`"), "{error}");
}

/// The same change as one replace leaves the plan valid at every entry.
#[test]
fn the_same_change_as_one_replace_is_accepted() {
    let successor = contract_value(json!({"id": "rejects-y", "file": "tests/y_contract.rs",
                                          "test": "tests::rejects_y"}));
    let patch = json!([{"field": "contracts", "op": "replace", "index": 0, "value": successor}]);
    let fx = fixture();
    let Ok(DisputeVerdict::Accept { plan_patch, .. }) =
        check(&fx.work, &fx.stage, &accepting(patch))
    else {
        panic!("a replace of the sole contract records");
    };
    let entries = plan_patch.inner["patches"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["op"], "replace");
}

/// An insert under the old contract's id is a duplicate until the delete
/// that follows it, so the apply refuses the insert.
#[test]
fn a_duplicate_id_insert_before_the_delete_of_the_old_contract_is_refused() {
    let twin = contract_value(json!({"file": "tests/y_contract.rs", "test": "tests::rejects_y"}));
    let patch = json!([
        {"field": "contracts", "op": "insert", "index": 0, "value": twin},
        {"field": "contracts", "op": "delete", "index": 1},
    ]);
    let fx = fixture();
    let error = check(&fx.work, &fx.stage, &accepting(patch)).unwrap_err();
    assert!(
        error.contains("after plan_patch.patches[0] (insert contracts[0])"),
        "{error}"
    );
    assert!(error.contains("id 'rejects-x' is used twice"), "{error}");
}

#[test]
fn a_contract_runner_no_adapter_answers_to_is_refused() {
    let fx = fixture();
    let unknown = contract_value(json!({"id": "rejects-y", "file": "tests/y_contract.rs",
                                        "runner": "no-such-runner"}));
    let patch = json!([{"field": "contracts", "op": "insert", "index": 1, "value": unknown}]);
    refused(&fx, accepting(patch), "names runner `no-such-runner`");
}

#[test]
fn a_valid_accept_is_recorded_in_its_canonical_order() {
    let fx = fixture();
    let patches = json!([
        {"field": "harness", "op": "insert", "index": 0, "value": " tests/fixtures/** "},
        {"field": "acceptance", "patch": {"op": "replace", "index": 1,
         "value": "cargo fmt --all -- --check"}},
    ]);
    let Ok(DisputeVerdict::Accept { plan_patch, .. }) =
        check(&fx.work, &fx.stage, &accepting(patches))
    else {
        panic!("a valid accept records");
    };
    let entries = plan_patch.inner["patches"].as_array().unwrap().clone();
    assert_eq!(entries[0]["field"], "harness");
    assert_eq!(entries[0]["op"], "insert");
    assert_eq!(entries[1]["field"], "acceptance");
    assert_eq!(entries[1]["value"], "cargo fmt --all -- --check");
}

#[test]
fn without_its_stage_a_freeze_refusal_verdict_is_never_recorded_as_given() {
    let raw = json!({"verdict": "reject", "reasoning": "r", "citations": citation(),
                     "instructions": "i"});
    let asked = question(parse_and_validate(&raw.to_string(), &refusal()));
    assert!(asked.contains("checked against its stage"), "{asked}");
}
