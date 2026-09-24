//! Plan `version: 2` rules. A v1 plan may not use a v2-only field (DESIGN D1),
//! a v2 plan's v2 fields must be well formed (D3), and on a v2 plan a
//! `plan verify` lint marked `error_in_v2` is an error rather than a warning (D4).

use std::collections::HashSet;
use std::path::{Component, Path};

use super::super::detect::detect_stage_type;
use super::super::types::{
    ContractSpec, LoomMetadata, ReachableCheck, StageDefinition, StageType, ValidationError,
};
use super::v2_lints::LintFinding;

/// Push the errors the v2-only fields raise for the plan's version: every use
/// on a v1 plan, the D3 rules on a v2 plan. Any other version is already an
/// error of its own and gets neither.
pub(super) fn push_v2_field_errors(metadata: &LoomMetadata, errors: &mut Vec<ValidationError>) {
    match metadata.loom.version {
        1 => push_v1_uses(metadata, errors),
        2 => push_v2_rules(metadata, errors),
        _ => {}
    }
}

/// Split `plan verify` lint findings by severity for a plan of `version`: on a
/// v2 plan a finding marked `error_in_v2` is an error; every other finding is a
/// structural warning, stage-prefixed like the other structural warnings.
pub(crate) fn split_lint_findings(
    version: u32,
    findings: Vec<LintFinding>,
) -> (Vec<ValidationError>, Vec<String>) {
    let is_v2 = version == 2;
    let (mut errors, mut warnings) = (Vec::new(), Vec::new());
    for finding in findings {
        let error = ValidationError {
            message: finding.message,
            stage_id: finding.stage_id,
        };
        if is_v2 && finding.error_in_v2 {
            errors.push(error);
        } else {
            warnings.push(error.to_string());
        }
    }
    (errors, warnings)
}

/// One `` `<field>` requires `version: 2` `` error per v2-only field a v1 plan uses.
fn push_v1_uses(metadata: &LoomMetadata, errors: &mut Vec<ValidationError>) {
    let requires_v2 = |field: &str, stage_id: Option<&str>| ValidationError {
        message: format!("{field} requires `version: 2`"),
        stage_id: stage_id.map(str::to_string),
    };
    if !metadata.loom.ratchet_files.is_empty() {
        errors.push(requires_v2("`ratchet_files`", None));
    }
    for stage in &metadata.loom.stages {
        let stage_id = Some(stage.id.as_str());
        for (field, empty) in [
            ("`contracts`", stage.contracts.is_empty()),
            ("`harness`", stage.harness.is_empty()),
            ("`reachable`", stage.reachable.is_empty()),
        ] {
            if !empty {
                errors.push(requires_v2(field, stage_id));
            }
        }
        for (idx, _) in stage.wiring.iter().enumerate().filter(|(_, w)| w.literal) {
            errors.push(requires_v2(
                &format!("Wiring #{} `literal`", idx + 1),
                stage_id,
            ));
        }
    }
}

/// The D3 rules for a v2 plan's `ratchet_files` and each stage's `contracts`,
/// `harness` and `reachable`.
fn push_v2_rules(metadata: &LoomMetadata, errors: &mut Vec<ValidationError>) {
    for path in &metadata.loom.ratchet_files {
        if let Some(problem) = path_problem(path) {
            errors.push(ValidationError {
                message: format!("ratchet_files entry '{path}' {problem}"),
                stage_id: None,
            });
        }
    }
    for stage in &metadata.loom.stages {
        let mut messages: Vec<String> = contract_count_problem(stage).into_iter().collect();
        push_contract_problems(&stage.contracts, &mut messages);
        for entry in &stage.harness {
            if let Some(problem) = path_problem(entry) {
                messages.push(format!("harness entry '{entry}' {problem}"));
            }
        }
        push_reachable_problems(&stage.reachable, &mut messages);
        errors.extend(messages.into_iter().map(|message| ValidationError {
            message,
            stage_id: Some(stage.id.clone()),
        }));
    }
}

/// A `standard` stage needs at least one contract; no other stage type takes any.
fn contract_count_problem(stage: &StageDefinition) -> Option<String> {
    let is_standard = detect_stage_type(stage) == StageType::Standard;
    match (is_standard, stage.contracts.is_empty()) {
        (true, true) => Some(
            "standard stages in a `version: 2` plan need at least one entry in `contracts`"
                .to_string(),
        ),
        (false, false) => Some(
            "only standard stages take `contracts`; knowledge, knowledge-distill and \
             integration-verify stages have none"
                .to_string(),
        ),
        _ => None,
    }
}

/// Contract `id` pattern and uniqueness, non-empty fields, and a relative `file`.
fn push_contract_problems(contracts: &[ContractSpec], messages: &mut Vec<String>) {
    let mut seen = HashSet::new();
    for (idx, contract) in contracts.iter().enumerate() {
        let label = format!("Contract #{}", idx + 1);
        if !is_contract_id(&contract.id) {
            messages.push(format!(
                "{label} id '{}' must match ^[a-z0-9][a-z0-9-]*$",
                contract.id
            ));
        } else if !seen.insert(contract.id.as_str()) {
            messages.push(format!("{label} id '{}' is used twice", contract.id));
        }
        let fields = [
            ("file", contract.file.as_str()),
            ("test", contract.test.as_str()),
            ("scenario", contract.scenario.as_str()),
            ("rejects", contract.rejects.as_str()),
        ];
        push_blank_fields(&label, &fields, messages);
        if let Some(problem) = path_problem(&contract.file) {
            messages.push(format!("{label} file '{}' {problem}", contract.file));
        }
    }
}

/// Non-empty `symbol`, `from` and `description`; `min_confidence` within `0.0..=1.0`.
fn push_reachable_problems(checks: &[ReachableCheck], messages: &mut Vec<String>) {
    for (idx, check) in checks.iter().enumerate() {
        let label = format!("Reachable #{}", idx + 1);
        let fields = [
            ("symbol", check.symbol.as_str()),
            ("from", check.from.as_str()),
            ("description", check.description.as_str()),
        ];
        push_blank_fields(&label, &fields, messages);
        if let Some(confidence) = check.min_confidence.filter(|c| !(0.0..=1.0).contains(c)) {
            messages.push(format!(
                "{label} min_confidence {confidence} is outside 0.0..=1.0"
            ));
        }
    }
}

/// One "`<field>` cannot be empty" message per blank field.
fn push_blank_fields(label: &str, fields: &[(&str, &str)], messages: &mut Vec<String>) {
    for (field, value) in fields {
        if value.trim().is_empty() {
            messages.push(format!("{label} `{field}` cannot be empty"));
        }
    }
}

/// `^[a-z0-9][a-z0-9-]*$`
fn is_contract_id(id: &str) -> bool {
    let lower_alnum = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let mut chars = id.chars();
    chars.next().is_some_and(lower_alnum) && chars.all(|c| lower_alnum(c) || c == '-')
}

/// Why `path` is not a relative path free of `..` components, or `None` when it is.
fn path_problem(path: &str) -> Option<&'static str> {
    let path = Path::new(path);
    if path.has_root() {
        Some("must be a relative path")
    } else if path.components().any(|c| c == Component::ParentDir) {
        Some("cannot contain a `..` component")
    } else {
        None
    }
}
