//! A criterion dispute over a wiring test, end to end: an accepted verdict
//! amends the plan's and the stage's `wiring_tests`.

use std::path::{Path, PathBuf};

use super::tests::{make_stage, write_request, write_stage, write_verdict};
use super::AdjudicatorRegistry;
use crate::models::dispute::{Citation, CriterionField, DisputeKind, DisputeVerdict, PlanPatch};
use crate::models::stage::WiringTest;
use crate::verify::transitions::load_stage;

/// A plan whose stage `s1` has one wiring test, between the metadata markers
/// `apply_amendment` splices the amended YAML into.
const PLAN: &str = r#"# Plan

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  stages:
    - id: s1
      name: s1
      working_dir: "."
      acceptance:
        - "true"
      wiring_tests:
        - name: "old smoke"
          command: "false"
```

<!-- END loom METADATA -->
"#;

/// `root/.loom/work`, its `config.toml` naming the plan written under
/// `root/doc/plans`. Returns the work dir and the plan path.
fn setup(root: &Path) -> (PathBuf, PathBuf) {
    let work = root.join(".loom").join("work");
    std::fs::create_dir_all(work.join("stages")).unwrap();
    let plans = root.join("doc").join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    let plan = plans.join("PLAN-wiring-tests.md");
    std::fs::write(&plan, PLAN).unwrap();
    let config = format!(
        "[plan]\nsource_path = \"{}\"\nplan_id = \"x\"\nplan_name = \"x\"\n\
         base_branch = \"main\"\n",
        plan.display()
    );
    std::fs::write(work.join("config.toml"), config).unwrap();
    (work, plan)
}

/// An accept verdict replacing wiring test 0 with `new smoke`.
fn accept_replacing_the_wiring_test() -> DisputeVerdict {
    DisputeVerdict::Accept {
        plan_patch: PlanPatch {
            inner: serde_json::json!({
                "field": "wiring-tests",
                "patch": {
                    "op": "replace",
                    "index": 0,
                    "value": "name: new smoke\ncommand: \"true\"\n",
                },
                "reason": "the wiring test runs a command that cannot pass",
            }),
        },
        citations: vec![Citation {
            file: "PLAN.md".to_string(),
            line: None,
            excerpt: "command: \"false\"".to_string(),
            claim: "`false` exits 1 on any tree".to_string(),
        }],
        reasoning: "no implementation can make `false` exit 0".to_string(),
    }
}

#[test]
fn an_accepted_wiring_tests_verdict_amends_the_plan_and_the_stage() {
    let tmp = tempfile::tempdir().unwrap();
    let (work, plan) = setup(tmp.path());
    let mut stage = make_stage("s1");
    stage.wiring_tests = vec![WiringTest {
        name: "old smoke".to_string(),
        command: "false".to_string(),
        success_criteria: Default::default(),
        description: None,
    }];
    write_stage(&work, &stage);
    let kind = DisputeKind::criterion(CriterionField::WiringTests, vec![0]);
    write_request(&work, "s1", 1, kind);
    write_verdict(&work, "s1", 1, accept_replacing_the_wiring_test(), 1);

    AdjudicatorRegistry::new()
        .apply_verdict(&work, "s1", 1)
        .unwrap();

    let amended_plan = std::fs::read_to_string(&plan).unwrap();
    assert!(amended_plan.contains("new smoke"), "{amended_plan}");
    assert!(!amended_plan.contains("old smoke"), "{amended_plan}");
    let stage = load_stage("s1", &work).unwrap();
    assert_eq!(stage.wiring_tests.len(), 1);
    assert_eq!(stage.wiring_tests[0].name, "new smoke");
    assert_eq!(stage.wiring_tests[0].command, "true");
}
