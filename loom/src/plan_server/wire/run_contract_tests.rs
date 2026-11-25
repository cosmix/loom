//! Contract test: a run start request carries the plan and the clean flag,
//! and nothing else.

use crate::plan_server::wire::run::RunStartRequest;

#[test]
fn run_start_request_is_plan_and_clean() {
    let request: RunStartRequest =
        serde_json::from_str(r#"{"plan":"doc/plans/PLAN-x.md","clean":true}"#)
            .expect("plan and clean alone must deserialize");
    assert_eq!(request.plan, "doc/plans/PLAN-x.md");
    assert!(request.clean);

    let value = serde_json::to_value(RunStartRequest {
        plan: "doc/plans/PLAN-x.md".into(),
        clean: false,
    })
    .expect("serialize RunStartRequest");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("RunStartRequest serializes to an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["clean", "plan"]);

    assert!(
        serde_json::from_str::<RunStartRequest>(
            r#"{"plan":"doc/plans/PLAN-x.md","clean":false,"backend":"tmux"}"#
        )
        .is_err(),
        "a body naming backend must not deserialize"
    );
}
