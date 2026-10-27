//! Tests for plan-definition conversion and the other `Stage` methods.

use super::*;
use crate::models::stage::{
    CommandConfinement, DeadCodeCheck, ExecutionMode, Implementer, Implementers, PermissionMode,
    RegressionTest, StageSandboxConfig, StageType, SuccessCriteria, TruthCheck, WiringCheck,
    WiringTest,
};
use crate::plan::schema::CodeReviewConfig;

const POLICY_PLAN: PlanIdentity<'static> = PlanIdentity {
    id: "plan-policy",
    version: 1,
    ratchet_files: &[],
};

fn truth_check(command: &str, description: &str) -> TruthCheck {
    TruthCheck {
        command: command.to_string(),
        stdout_contains: vec![],
        stdout_not_contains: vec![],
        stderr_empty: Some(true),
        exit_code: Some(0),
        timeout_secs: None,
        description: Some(description.to_string()),
    }
}

#[test]
fn from_definition_copies_all_runtime_policy_fields() {
    let definition = StageDefinition {
        id: "policy-stage".to_string(),
        name: "Policy Stage".to_string(),
        description: Some("full conversion".to_string()),
        summary: Some("converts every policy field".to_string()),
        dependencies: vec!["bootstrap".to_string()],
        parallel_group: Some("policy".to_string()),
        acceptance: vec![AcceptanceCriterion::Simple("cargo test".to_string())],
        setup: vec!["cargo build".to_string()],
        files: vec!["src/policy.rs".to_string()],
        auto_merge: Some(true),
        working_dir: "loom".to_string(),
        stage_type: Some(StageType::IntegrationVerify),
        artifacts: vec!["target/policy".to_string()],
        wiring: vec![WiringCheck {
            source: "src/lib.rs".to_string(),
            pattern: "policy".to_string(),
            description: "policy is exported".to_string(),
            literal: false,
        }],
        wiring_tests: vec![WiringTest {
            name: "runtime policy".to_string(),
            command: "cargo test policy".to_string(),
            success_criteria: SuccessCriteria {
                exit_code: Some(0),
                ..SuccessCriteria::default()
            },
            description: Some("runtime wiring".to_string()),
        }],
        dead_code_check: Some(DeadCodeCheck {
            command: "cargo check".to_string(),
            fail_patterns: vec!["unused".to_string()],
            ignore_patterns: vec!["fixture".to_string()],
        }),
        before_stage: vec![truth_check("test ! -e target/policy", "absent before")],
        after_stage: vec![truth_check("test -e target/policy", "present after")],
        context_ceiling_tokens: Some(71),
        removed_context_budget: None,
        plan_overview: Some(true),
        sandbox: StageSandboxConfig {
            enabled: Some(true),
            auto_allow: Some(false),
            allow_unsandboxed_escape: Some(false),
            excluded_commands: vec!["danger".to_string()],
            permission_mode: Some(PermissionMode::Plan),
            command_confinement: Some(CommandConfinement::Inherit),
            ..StageSandboxConfig::default()
        },
        execution_mode: Some(ExecutionMode::Team),
        bug_fix: Some(true),
        regression_test: Some(RegressionTest {
            file: "tests/policy.rs".to_string(),
            must_contain: vec!["regression".to_string()],
        }),
        model: Some("opus".to_string()),
        reasoning_effort: Some("xhigh".to_string()),
        code_review: Some(CodeReviewConfig {
            dimensions: vec!["security".to_string(), "wiring".to_string()],
            require_all: true,
        }),
        ultracode: true,
        implementers: Implementers::new(vec![Implementer::Codex, Implementer::Claude]),
        subagent_timeout_secs: Some(900),
        skills: vec!["loom-rust".to_string()],
        ..Default::default()
    };

    let stage = Stage::from_definition(&definition, &POLICY_PLAN);

    assert_eq!(stage.id, definition.id);
    assert_eq!(stage.name, definition.name);
    assert_eq!(stage.description, definition.description);
    assert_eq!(stage.summary, definition.summary);
    assert_eq!(stage.status, StageStatus::WaitingForDeps);
    assert_eq!(stage.dependencies, definition.dependencies);
    assert_eq!(stage.parallel_group, definition.parallel_group);
    assert_eq!(stage.acceptance, definition.acceptance);
    assert_eq!(stage.setup, definition.setup);
    assert_eq!(stage.files, definition.files);
    assert_eq!(stage.stage_type, StageType::IntegrationVerify);
    assert_eq!(stage.plan_id.as_deref(), Some("plan-policy"));
    assert_eq!(stage.auto_merge, Some(true));
    assert_eq!(stage.working_dir.as_deref(), Some("loom"));
    assert_eq!(stage.context_ceiling_tokens, Some(71));
    assert_eq!(stage.plan_overview, Some(true));
    assert_eq!(stage.artifacts, definition.artifacts);
    assert_eq!(stage.wiring[0].source, "src/lib.rs");
    assert_eq!(stage.wiring_tests[0].command, "cargo test policy");
    assert_eq!(
        stage.dead_code_check.as_ref().unwrap().command,
        "cargo check"
    );
    assert_eq!(stage.before_stage[0].exit_code, Some(0));
    assert_eq!(stage.after_stage[0].exit_code, Some(0));
    assert_eq!(stage.sandbox.enabled, Some(true));
    assert_eq!(stage.sandbox.permission_mode, Some(PermissionMode::Plan));
    assert_eq!(
        stage.sandbox.command_confinement,
        Some(CommandConfinement::Inherit)
    );
    assert_eq!(stage.execution_mode, Some(ExecutionMode::Team));
    assert_eq!(stage.bug_fix, Some(true));
    assert_eq!(
        stage.regression_test.as_ref().unwrap().file,
        "tests/policy.rs"
    );
    assert_eq!(stage.model.as_deref(), Some("opus"));
    assert_eq!(stage.reasoning_effort.as_deref(), Some("xhigh"));
    assert_eq!(
        stage.code_review.as_ref().unwrap().dimensions,
        vec!["security".to_string(), "wiring".to_string()]
    );
    assert!(stage.ultracode);
    assert_eq!(stage.implementers.preferred(), Implementer::Codex);
    assert_eq!(stage.subagent_timeout_secs, Some(900));
    assert_eq!(stage.skills, definition.skills);
}
