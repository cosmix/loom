//! Contracts for stage `refusal-adjudication`: who may file a
//! `freeze-refusal` dispute, and a harness amendment that reaches both the
//! plan file and the stage record.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use loom::models::dispute::DisputeKind;
use loom::models::session::SessionType;
use loom::models::stage::Stage;
use loom::plan::schema::{AcceptanceCriterion, ContractSpec};
use loom::plan::{apply_amendment, AmendmentField, AmendmentPatch, AmendmentRequest};
use loom::verify::{load_stage, save_stage};

const STAGE_ID: &str = "stage-h";

/// A v2 plan whose standard stage carries one contract and no harness.
const PLAN_CONTENT: &str = "# PLAN: Harness Amendment Contract\n\n\
Some prose.\n\n\
<!-- loom METADATA -->\n\n\
```yaml\n\
loom:\n  version: 2\n  stages:\n    - id: stage-h\n      name: \"Harness\"\n      stage_type: standard\n      working_dir: \".\"\n      dependencies: []\n      contracts:\n        - id: rejects-x\n          file: \"tests/x_contract.rs\"\n          test: \"rejects_x\"\n          runner: cargo-test\n          scenario: \"x arrives\"\n          rejects: \"accepting x\"\n      acceptance:\n        - \"cargo test\"\n```\n\n\
<!-- END loom METADATA -->\n";

struct Env {
    _tmp: TempDir,
    work_dir: PathBuf,
    plan_path: PathBuf,
}

fn contract() -> ContractSpec {
    ContractSpec {
        id: "rejects-x".to_string(),
        file: "tests/x_contract.rs".to_string(),
        test: "rejects_x".to_string(),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: "x arrives".to_string(),
        rejects: "accepting x".to_string(),
    }
}

fn setup() -> Env {
    let tmp = TempDir::new().unwrap();
    let project_root = tmp.path().to_path_buf();
    let work_dir = project_root.join(".loom").join("work");
    fs::create_dir_all(work_dir.join("stages")).unwrap();
    fs::create_dir_all(project_root.join("doc").join("plans")).unwrap();
    let plan_path = project_root
        .join("doc")
        .join("plans")
        .join("PLAN-harness.md");
    fs::write(&plan_path, PLAN_CONTENT).unwrap();

    let cfg = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"X\"\nbase_branch = \"main\"\n",
        plan_path.display(),
    );
    fs::write(work_dir.join("config.toml"), cfg).unwrap();

    let stage = Stage {
        id: STAGE_ID.to_string(),
        name: "Harness".to_string(),
        working_dir: Some(".".to_string()),
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        plan_version: 2,
        contracts: vec![contract()],
        ..Stage::default()
    };
    save_stage(&stage, &work_dir).unwrap();

    Env {
        _tmp: tmp,
        work_dir,
        plan_path,
    }
}

#[test]
fn refusal_dispute_filed_only_by_contract_writer() {
    let refusal = DisputeKind::FreezeRefusal {
        problems: vec!["x".into()],
    };
    assert!(
        refusal.filed_by(SessionType::Contract),
        "the contract writer's refusal is filed as a freeze-refusal dispute"
    );
    assert!(
        !refusal.filed_by(SessionType::Stage),
        "an implementer must not file a freeze-refusal dispute"
    );

    let contract = DisputeKind::Contract {
        contract_id: "c".into(),
    };
    assert!(
        !contract.filed_by(SessionType::Contract),
        "a contract writer must not dispute its own frozen contracts"
    );
    assert!(
        contract.filed_by(SessionType::Stage),
        "the implementer disputes a frozen contract"
    );
}

#[test]
fn harness_amendment_reaches_stage_record() {
    let env = setup();
    let request = AmendmentRequest {
        stage_id: STAGE_ID.to_string(),
        field: AmendmentField::Harness,
        patch: AmendmentPatch::Insert {
            index: 0,
            value: "tests/fixtures/**".into(),
        },
        reason: Some("harness amendment".to_string()),
        dispute_id: None,
    };
    apply_amendment(&env.plan_path, &env.work_dir, request).expect("harness amendment applies");

    let stage = load_stage(STAGE_ID, &env.work_dir).unwrap();
    assert_eq!(
        stage.harness,
        vec!["tests/fixtures/**".to_string()],
        "the stage record carries the amended harness globs"
    );
    let plan = fs::read_to_string(&env.plan_path).unwrap();
    assert!(
        plan.contains("tests/fixtures/**"),
        "the plan file carries the amended harness glob:\n{plan}"
    );
}
