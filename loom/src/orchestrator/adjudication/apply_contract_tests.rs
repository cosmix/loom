//! An accepted contract dispute's re-freeze (`apply_contract.rs`): which run
//! directory each contract keeps or gets and which files are re-hashed, with
//! and without an amendment, on a retry that finds the amendment applied, and
//! for harness files changed, added or deleted since the freeze.

use chrono::Utc;
use serde_json::json;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

use super::tests::{make_stage, write_request, write_stage, write_verdict};
use super::AdjudicatorRegistry;
use crate::fs::work_dir::WorkDir;
use crate::models::dispute::{Citation, DisputeKind, DisputeVerdict, PlanPatch};
use crate::models::stage::Stage;
use crate::plan::schema::ContractSpec;
use crate::relay::sha256_hex;
use crate::verify::contracts::store::{
    self as contract_store, FreezeRecord, FrozenContract, FrozenFile, REFROZEN_OUTCOME,
};
use crate::verify::contracts::test_support::{
    contract, contract_worktree, red_reports, CONTRACT_CONTENT, CONTRACT_FILE, CONTRACT_ID,
};
use crate::verify::transitions::update_stage;

/// A v2 plan whose stage `s1` holds the one contract of [`contract`].
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
```

<!-- END loom METADATA -->
"#;

struct Fixture {
    _tmp: TempDir,
    work: PathBuf,
    worktree: PathBuf,
}

/// The harness file [`fixture_with_harness`] freezes.
const HARNESS_FILE: &str = "tests/support/helper.rs";
const HARNESS_CONTENT: &[u8] = b"pub fn ready() -> bool { false }\n";

/// A stage worktree holding the contract file, its state directory naming
/// [`PLAN`], and the stage under adjudication, its contract frozen with the
/// run directory `cwd` recorded for its file.
fn fixture(cwd: Option<&str>) -> Fixture {
    fixture_with(cwd, &[])
}

/// [`fixture`] with [`HARNESS_FILE`] in the worktree and in the freeze.
fn fixture_with_harness() -> Fixture {
    fixture_with(Some("."), &[(HARNESS_FILE, HARNESS_CONTENT)])
}

/// [`fixture`] with each of `harness` written to the worktree and frozen
/// beside the contract file.
fn fixture_with(cwd: Option<&str>, harness: &[(&str, &[u8])]) -> Fixture {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    let worktree = contract_worktree(&repo, "s1");
    std::fs::create_dir_all(worktree.join("tests/support")).unwrap();
    let mut copies = vec![(CONTRACT_FILE.to_string(), CONTRACT_CONTENT.to_vec())];
    for (path, content) in harness {
        std::fs::write(worktree.join(path), content).unwrap();
        copies.push((path.to_string(), content.to_vec()));
    }
    let workspace = WorkDir::new(&repo).unwrap();
    workspace.adopt_existing().unwrap();
    let work = workspace.root().to_path_buf();
    write_plan(&repo, &work);
    write_stage(&work, &stage(vec![contract()]));
    let frozen = red_reports()
        .iter()
        .map(|report| FrozenContract {
            file: Some(CONTRACT_FILE.to_string()),
            cwd: cwd.map(str::to_string),
            ..FrozenContract::from(report)
        })
        .collect();
    let record = FreezeRecord {
        version: contract_store::FREEZE_RECORD_VERSION,
        stage_id: "s1".to_string(),
        session_id: "contract-session".to_string(),
        frozen_at: Utc::now(),
        base: "main".to_string(),
        files: copies
            .iter()
            .map(|(path, content)| FrozenFile {
                path: path.clone(),
                sha256: sha256_hex(content),
            })
            .collect(),
        contracts: frozen,
    };
    contract_store::write_freeze(&work, &record, &copies).unwrap();
    Fixture {
        _tmp: tmp,
        work,
        worktree,
    }
}

/// The stage under adjudication, holding `contracts`.
fn stage(contracts: Vec<ContractSpec>) -> Stage {
    Stage {
        plan_version: 2,
        worktree: Some("s1".to_string()),
        harness: vec!["tests/support/**".to_string()],
        contracts,
        ..make_stage("s1")
    }
}

/// Write [`PLAN`] under the repository's `doc/plans` and name it in `work`'s
/// config.
fn write_plan(repo: &Path, work: &Path) {
    let plans = repo.join("doc").join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    let plan = plans.join("PLAN-contracts.md");
    std::fs::write(&plan, PLAN).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\n\
         base_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work.join("config.toml"), config).unwrap();
}

/// Accept the dispute over [`CONTRACT_ID`] with `plan_patch`, and apply it.
fn accept(fx: &Fixture, plan_patch: serde_json::Value) {
    let kind = DisputeKind::Contract {
        contract_id: CONTRACT_ID.to_string(),
    };
    write_request(&fx.work, "s1", 1, kind);
    let verdict = DisputeVerdict::Accept {
        plan_patch: PlanPatch { inner: plan_patch },
        citations: vec![Citation {
            file: CONTRACT_FILE.to_string(),
            line: Some(1),
            excerpt: "fn rejects_x()".to_string(),
            claim: "the contract's spec is wrong".to_string(),
        }],
        reasoning: "the disputed contract is wrong".to_string(),
    };
    write_verdict(&fx.work, "s1", 1, verdict, 1);
    AdjudicatorRegistry::new()
        .apply_verdict(&fx.work, "s1", 1)
        .unwrap();
}

/// A `contracts` amendment replacing contract 0 with `spec` (YAML).
fn replacing_contract(spec: &str) -> serde_json::Value {
    json!({
        "field": "contracts",
        "patch": {"op": "replace", "index": 0, "value": spec},
        "reason": "the contract's spec is wrong",
    })
}

fn frozen(fx: &Fixture, id: &str) -> FrozenContract {
    let record = contract_store::load_freeze(&fx.work, "s1")
        .unwrap()
        .unwrap();
    let found = record.contracts.iter().find(|c| c.id == id).cloned();
    found.unwrap_or_else(|| panic!("no frozen contract '{id}': {record:?}"))
}

/// The hash the record holds for the frozen file `path`, if it lists it.
fn frozen_hash(fx: &Fixture, path: &str) -> Option<String> {
    let record = contract_store::load_freeze(&fx.work, "s1")
        .unwrap()
        .unwrap();
    let found = record.files.iter().find(|file| file.path == path);
    found.map(|file| file.sha256.clone())
}

/// Write `content` to `path` in the stage's worktree.
fn write_worktree_file(fx: &Fixture, path: &str, content: &[u8]) {
    std::fs::write(fx.worktree.join(path), content).unwrap();
}

/// Make `amended` the stage's one contract without touching the freeze
/// record: what a retry finds after a failure between the amendment and the
/// re-freeze.
fn amend_stage_only(fx: &Fixture, amended: ContractSpec) {
    update_stage("s1", &fx.work, |stage| {
        stage.contracts = vec![amended];
        Ok(())
    })
    .unwrap();
}

/// Give the contract file's directory a runner config of its own.
fn plant_nearer_marker(fx: &Fixture) {
    std::fs::write(fx.worktree.join("tests/Cargo.toml"), "").unwrap();
}

#[test]
fn an_amended_cwd_is_recorded() {
    let fx = fixture(Some("."));
    let spec = "id: rejects-x\nfile: tests/x_contract.rs\ntest: tests::rejects_x\n\
                runner: cargo-test\ncwd: tests\nscenario: x arrives\nrejects: accepting x\n";

    accept(&fx, replacing_contract(spec));

    assert_eq!(frozen(&fx, CONTRACT_ID).cwd.as_deref(), Some("tests"));
}

#[test]
fn an_accept_without_an_amendment_keeps_the_recorded_cwd() {
    let fx = fixture(Some("."));
    plant_nearer_marker(&fx);

    accept(&fx, json!({}));

    assert_eq!(frozen(&fx, CONTRACT_ID).cwd.as_deref(), Some("."));
}

#[test]
fn a_record_without_a_cwd_gets_one() {
    let fx = fixture(None);
    plant_nearer_marker(&fx);

    accept(&fx, json!({}));

    assert_eq!(frozen(&fx, CONTRACT_ID).cwd.as_deref(), Some("tests"));
}

#[test]
fn a_renamed_contract_keeps_the_cwd_recorded_for_its_file() {
    let fx = fixture(Some("."));
    plant_nearer_marker(&fx);
    let spec = "id: rejects-y\nfile: tests/x_contract.rs\ntest: tests::rejects_x\n\
                runner: cargo-test\nscenario: x arrives\nrejects: accepting x\n";

    accept(&fx, replacing_contract(spec));

    let added = frozen(&fx, "rejects-y");
    assert_eq!(added.cwd.as_deref(), Some("."));
    assert_eq!(added.file.as_deref(), Some(CONTRACT_FILE));
    assert_eq!(added.outcome, REFROZEN_OUTCOME);
    assert_eq!(added.adapter.as_deref(), Some("cargo-test"));
    assert_eq!(added.exit_code, None);
}

#[test]
fn an_inserted_contract_has_its_file_frozen() {
    let fx = fixture(Some("."));
    let content: &[u8] = b"#[test]\nfn rejects_z() { assert!(false); }\n";
    write_worktree_file(&fx, "tests/z_contract.rs", content);
    let spec = "id: rejects-z\nfile: tests/z_contract.rs\ntest: tests::rejects_z\n\
                runner: cargo-test\nscenario: z arrives\nrejects: accepting z\n";
    let insert = json!({
        "field": "contracts",
        "patch": {"op": "insert", "index": 1, "value": spec},
        "reason": "the stage needs a contract for z",
    });

    accept(&fx, insert);

    let expected = sha256_hex(content);
    assert_eq!(frozen_hash(&fx, "tests/z_contract.rs"), Some(expected));
    let added = frozen(&fx, "rejects-z");
    assert_eq!(added.file.as_deref(), Some("tests/z_contract.rs"));
    assert_eq!(added.outcome, REFROZEN_OUTCOME);
}

#[test]
fn a_retry_records_an_amended_cwd_the_record_lacks() {
    let fx = fixture(Some("."));
    let amended = ContractSpec {
        cwd: Some("tests".to_string()),
        ..contract()
    };
    amend_stage_only(&fx, amended);

    accept(&fx, json!({}));

    assert_eq!(frozen(&fx, CONTRACT_ID).cwd.as_deref(), Some("tests"));
}

#[test]
fn a_retry_records_and_freezes_an_amended_file_the_record_lacks() {
    let fx = fixture(Some("."));
    plant_nearer_marker(&fx);
    let moved: &[u8] = b"#[test]\nfn rejects_x() { assert!(reject_x()); }\n";
    write_worktree_file(&fx, "tests/y_contract.rs", moved);
    let amended = ContractSpec {
        file: "tests/y_contract.rs".to_string(),
        ..contract()
    };
    amend_stage_only(&fx, amended);

    accept(&fx, json!({}));

    let refrozen = frozen(&fx, CONTRACT_ID);
    assert_eq!(refrozen.file.as_deref(), Some("tests/y_contract.rs"));
    assert_eq!(refrozen.cwd.as_deref(), Some("tests"));
    let expected = sha256_hex(moved);
    assert_eq!(frozen_hash(&fx, "tests/y_contract.rs"), Some(expected));
    let original = sha256_hex(CONTRACT_CONTENT);
    assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(original));
}

#[test]
fn a_changed_harness_file_is_refrozen() {
    let fx = fixture_with_harness();
    let changed: &[u8] = b"pub fn ready() -> bool { true }\n";
    write_worktree_file(&fx, HARNESS_FILE, changed);

    accept(&fx, json!({}));

    assert_eq!(frozen_hash(&fx, HARNESS_FILE), Some(sha256_hex(changed)));
    let copy = contract_store::frozen_file_path(&fx.work, "s1", HARNESS_FILE);
    assert_eq!(std::fs::read(copy).unwrap(), changed);
    let contract = sha256_hex(CONTRACT_CONTENT);
    assert_eq!(frozen_hash(&fx, CONTRACT_FILE), Some(contract));
}

#[test]
fn a_new_harness_file_is_added() {
    let fx = fixture_with_harness();
    let added: &[u8] = b"pub const FIXTURE: &str = \"x\";\n";
    write_worktree_file(&fx, "tests/support/fixture.rs", added);

    accept(&fx, json!({}));

    let expected = sha256_hex(added);
    assert_eq!(frozen_hash(&fx, "tests/support/fixture.rs"), Some(expected));
    let kept = sha256_hex(HARNESS_CONTENT);
    assert_eq!(frozen_hash(&fx, HARNESS_FILE), Some(kept));
}

#[test]
fn a_deleted_harness_file_is_dropped() {
    let fx = fixture_with_harness();
    std::fs::remove_file(fx.worktree.join(HARNESS_FILE)).unwrap();

    accept(&fx, json!({}));

    assert_eq!(frozen_hash(&fx, HARNESS_FILE), None);
    let copy = contract_store::frozen_file_path(&fx.work, "s1", HARNESS_FILE);
    assert!(!copy.exists(), "the dropped file's copy must go");
    assert!(frozen_hash(&fx, CONTRACT_FILE).is_some());
}
