//! Plan `version: 2` validation: the version check (DESIGN D1), v2-only fields
//! on a v1 plan (D1), the v2 field rules (D3), and lint severity (D4).

use super::{create_valid_metadata, create_valid_metadata_v2, make_stage};
use crate::plan::schema::types::{
    ContractSpec, LoomMetadata, ReachableCheck, StageType, ValidationError, WiringCheck,
};
use crate::plan::schema::validation::v2_lints::LintFinding;
use crate::plan::schema::validation::{v2_fields::split_lint_findings, validate};
use crate::plan::schema::ProvisionEntry;

fn errors_of(metadata: &LoomMetadata) -> Vec<ValidationError> {
    validate(metadata).err().unwrap_or_default()
}

fn messages_of(metadata: &LoomMetadata) -> Vec<String> {
    errors_of(metadata)
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn contract(id: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        file: "tests/plan_v2.rs".to_string(),
        test: "parses_v2_plan".to_string(),
        runner: None,
        cwd: None,
        scenario: "a version 2 plan is parsed".to_string(),
        rejects: "a parser that accepts only version 1".to_string(),
    }
}

fn wiring(pattern: &str, literal: bool) -> WiringCheck {
    WiringCheck {
        source: "src/lib.rs".to_string(),
        pattern: pattern.to_string(),
        description: "the entry point calls run".to_string(),
        literal,
    }
}

#[test]
fn v1_plan_with_contracts_requires_version_2() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.version = 1;

    let errors = errors_of(&metadata);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].message, "`contracts` requires `version: 2`");
    assert_eq!(errors[0].stage_id.as_deref(), Some("stage-1"));
    assert_eq!(
        errors[0].to_string(),
        "Stage 'stage-1': `contracts` requires `version: 2`"
    );
}

#[test]
fn v2_standard_stage_without_contracts_is_rejected() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.stages[0].contracts.clear();

    let errors = errors_of(&metadata);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].stage_id.as_deref(), Some("stage-1"));
    assert!(errors[0]
        .message
        .contains("need at least one entry in `contracts`"));
}

#[test]
fn v2_plan_with_valid_contract_passes() {
    let metadata = create_valid_metadata_v2();

    let errors = errors_of(&metadata);

    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn unsupported_version_3_is_rejected() {
    let mut metadata = create_valid_metadata();
    metadata.loom.version = 3;

    let errors = errors_of(&metadata);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        "Unsupported version: 3. Supported versions: 1, 2."
    );
    assert!(errors[0].message.contains("Supported versions: 1, 2"));
}

#[test]
fn v1_plan_rejects_every_v2_only_field_use() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.version = 1;
    metadata.loom.ratchet_files = vec!["loom/maintainability-baseline.txt".to_string()];
    let stage = &mut metadata.loom.stages[0];
    stage.harness = vec!["tests/fixtures/**".to_string()];
    stage.reachable = vec![ReachableCheck {
        symbol: "parse_v2".to_string(),
        from: "main".to_string(),
        min_confidence: None,
        description: "the parser is reached from main".to_string(),
    }];
    // `run(` is not a valid regex; a literal pattern is never compiled as one.
    stage.wiring = vec![wiring("run(", false), wiring("run(", true)];

    let messages = messages_of(&metadata);

    assert_eq!(
        messages[..5],
        [
            "`ratchet_files` requires `version: 2`",
            "Stage 'stage-1': `contracts` requires `version: 2`",
            "Stage 'stage-1': `harness` requires `version: 2`",
            "Stage 'stage-1': `reachable` requires `version: 2`",
            "Stage 'stage-1': Wiring #2 `literal` requires `version: 2`",
        ]
    );
    assert_eq!(messages.len(), 6, "{messages:?}");
    assert!(messages[5].starts_with("Stage 'stage-1': Wiring #1 has invalid regex pattern"));
}

#[test]
fn v2_literal_wiring_pattern_is_not_compiled_as_a_regex() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.stages[0].wiring = vec![wiring("run(", true)];

    let errors = errors_of(&metadata);

    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn v2_non_standard_stage_with_contracts_is_rejected() {
    let mut metadata = create_valid_metadata_v2();
    let mut knowledge = make_stage("knowledge-bootstrap", "Knowledge");
    knowledge.stage_type = Some(StageType::Knowledge);
    knowledge.contracts = vec![contract("knowledge-contract")];
    metadata.loom.stages.push(knowledge);

    let errors = errors_of(&metadata);

    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].stage_id.as_deref(), Some("knowledge-bootstrap"));
    assert!(errors[0]
        .message
        .starts_with("only standard stages take `contracts`"));
}

#[test]
fn v2_malformed_contracts_are_rejected() {
    let mut metadata = create_valid_metadata_v2();
    let mut blank = contract("Bad_Id");
    blank.file = "../outside.rs".to_string();
    blank.test = " ".to_string();
    metadata.loom.stages[0].contracts = vec![contract("dup"), contract("dup"), blank];

    let messages = messages_of(&metadata);

    assert_eq!(
        messages,
        [
            "Stage 'stage-1': Contract #2 id 'dup' is used twice",
            "Stage 'stage-1': Contract #3 id 'Bad_Id' must match ^[a-z0-9][a-z0-9-]*$",
            "Stage 'stage-1': Contract #3 `test` cannot be empty",
            "Stage 'stage-1': Contract #3 file '../outside.rs' cannot contain a `..` component",
        ]
    );
}

#[test]
fn v2_reachable_harness_and_ratchet_rules_are_enforced() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.ratchet_files = vec!["/etc/baseline.txt".to_string(), "ok.txt".to_string()];
    let stage = &mut metadata.loom.stages[0];
    stage.harness = vec!["tests/**".to_string(), "fixtures/../../x".to_string()];
    stage.reachable = vec![ReachableCheck {
        symbol: String::new(),
        from: "main".to_string(),
        min_confidence: Some(1.5),
        description: "reached".to_string(),
    }];

    let messages = messages_of(&metadata);

    assert_eq!(
        messages,
        [
            "ratchet_files entry '/etc/baseline.txt' must be a relative path",
            "Stage 'stage-1': harness entry 'fixtures/../../x' cannot contain a `..` component",
            "Stage 'stage-1': Reachable #1 `symbol` cannot be empty",
            "Stage 'stage-1': Reachable #1 min_confidence 1.5 is outside 0.0..=1.0",
        ]
    );
}

#[test]
fn lint_findings_split_by_plan_version() {
    let findings = || {
        vec![
            LintFinding {
                stage_id: Some("stage-1".to_string()),
                message: "unknown loom subcommand".to_string(),
                error_in_v2: true,
            },
            LintFinding {
                stage_id: None,
                message: "plan-wide".to_string(),
                error_in_v2: false,
            },
        ]
    };

    let (errors, warnings) = split_lint_findings(2, findings());
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].to_string(),
        "Stage 'stage-1': unknown loom subcommand"
    );
    assert_eq!(warnings, ["plan-wide"]);

    let (errors, warnings) = split_lint_findings(1, findings());
    assert!(errors.is_empty());
    assert_eq!(
        warnings,
        ["Stage 'stage-1': unknown loom subcommand", "plan-wide"]
    );
}

fn provision(working_dir: &str, command: &str) -> ProvisionEntry {
    ProvisionEntry {
        working_dir: working_dir.to_string(),
        command: command.to_string(),
    }
}

#[test]
fn v1_plan_with_provision_requires_version_2() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.version = 1;
    metadata.loom.stages[0].contracts.clear();
    metadata.loom.provision = vec![provision("web", "bun install")];

    assert_eq!(
        messages_of(&metadata),
        ["`provision` requires `version: 2`"]
    );
}

#[test]
fn v2_provision_entries_with_bad_fields_are_rejected() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.provision = vec![
        provision("../outside", "true"),
        provision("/abs/web", "true"),
        provision("  ", "true"),
        provision("web", " "),
    ];

    assert_eq!(
        messages_of(&metadata),
        [
            "provision entry #1 working_dir '../outside' cannot contain a `..` component",
            "provision entry #2 working_dir '/abs/web' must be a relative path",
            "provision entry #3 working_dir must not be empty (use \".\" for the repository root)",
            "provision entry #4 has an empty command",
        ]
    );
}

#[test]
fn v2_valid_provision_entries_pass() {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.provision = vec![
        provision("web", BUN_HARDENED),
        provision(".", "uv sync --frozen --no-install-project"),
    ];

    let errors = errors_of(&metadata);
    assert!(errors.is_empty(), "{errors:?}");
}

const BUN_HARDENED: &str = "test ! -e .npmrc && test ! -L .npmrc && bun install \
                            --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null";

/// The messages a v2 plan whose only provision entry runs `command` raises.
pub(super) fn provision_messages(command: &str) -> Vec<String> {
    let mut metadata = create_valid_metadata_v2();
    metadata.loom.provision = vec![provision("web", command)];
    messages_of(&metadata)
}

#[test]
fn an_unhardened_bun_install_names_every_missing_guard() {
    let messages = provision_messages("bun install --frozen-lockfile");

    assert_eq!(messages.len(), 1, "{messages:?}");
    let message = &messages[0];
    assert!(
        message.starts_with("provision entry #1 runs `bun install` without `--ignore-scripts`"),
        "{message}"
    );
    for missing in [
        "`--backend=copyfile`",
        "`--config=/dev/null`",
        "test ! -L .npmrc",
    ] {
        assert!(message.contains(missing), "{missing}: {message}");
    }
    assert!(message.contains(BUN_HARDENED), "{message}");
    assert!(!message.contains("`web`"), "{message}");
}

#[test]
fn hardened_installs_and_commands_without_an_installer_are_valid() {
    for command in [
        BUN_HARDENED,
        "test ! -L .npmrc && test ! -e .npmrc &&\n  bun i --ignore-scripts --backend copyfile \
         --config /dev/null\n",
        "test ! -e .npmrc && test ! -L .npmrc && npm ci --ignore-scripts",
        "test ! -e .npmrc && test ! -L .npmrc && pnpm install --frozen-lockfile \
         --ignore-scripts --ignore-pnpmfile",
        "test ! -e .npmrc && test ! -L .npmrc && yarn --frozen-lockfile --ignore-scripts",
        "uv sync --frozen --no-install-project",
        "true",
        "pwd > provisioned.txt",
    ] {
        let messages = provision_messages(command);
        assert!(messages.is_empty(), "{command:?}: {messages:?}");
    }
}

#[test]
fn installs_missing_a_flag_or_a_gating_npmrc_refusal_are_errors() {
    let cases = [
        (
            "uv sync --frozen",
            "`uv sync` without `--no-install-project`",
        ),
        (
            "test ! -e .npmrc && test ! -L .npmrc && pnpm i --ignore-scripts",
            "`pnpm i` without `--ignore-pnpmfile`",
        ),
        (
            "npm ci --ignore-scripts",
            "`npm ci` without a leading `test ! -e .npmrc",
        ),
        (
            "test ! -e .npmrc; test ! -L .npmrc; npm ci --ignore-scripts",
            "`npm ci` without a leading",
        ),
        (
            "test ! -e .npmrc || true && test ! -L .npmrc && npm ci --ignore-scripts",
            "`npm ci` without a leading",
        ),
        (
            "sh -c 'yarn install'",
            "`yarn install` without `--ignore-scripts`",
        ),
    ];
    for (command, expected) in cases {
        let messages = provision_messages(command);
        assert_eq!(messages.len(), 1, "{command:?}: {messages:?}");
        assert!(
            messages[0].starts_with(&format!("provision entry #1 runs {expected}")),
            "{command:?}: {messages:?}"
        );
    }
}
