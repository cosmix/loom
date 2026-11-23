use super::{make_stage, LoomConfig, LoomMetadata};
use crate::plan::schema::{validate, RegressionTest, WiringCheck};

fn plan(path: &str) -> LoomMetadata {
    let mut stage = make_stage("stage-1", "Stage One");
    stage.working_dir = path.into();
    stage.artifacts = vec![path.into()];
    stage.wiring = vec![WiringCheck {
        source: path.into(),
        pattern: "fn main".into(),
        description: "Verify the entry point is wired".into(),
        literal: false,
    }];
    stage.bug_fix = Some(true);
    stage.regression_test = Some(RegressionTest {
        file: path.into(),
        must_contain: vec!["test_bug_fixed".into()],
    });
    LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages: vec![stage],
            ..Default::default()
        },
    }
}

#[test]
fn embedded_dotdot_is_accepted_in_each_path_field() {
    for path in ["foo..bar.rs", "out/a..b"] {
        let result = validate(&plan(path));
        assert!(result.is_ok(), "{path}: {result:?}");
    }
}

#[test]
fn parent_components_are_rejected_in_each_path_field() {
    for path in ["../x", "a/../b"] {
        let errors = validate(&plan(path)).expect_err("parent components must be rejected");
        let messages: Vec<_> = errors.iter().map(|error| error.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "working_dir cannot contain path traversal (..)",
                "Artifact #1 contains path traversal (..)",
                "Wiring #1 source contains path traversal (..)",
                "regression_test.file cannot contain path traversal (..)",
            ],
            "{path}"
        );
    }
}
