//! Bookend lints: one knowledge-distill stage last in the plan, every stage an
//! ancestor of it, and every standard stage an ancestor of an integration-verify stage.

use crate::plan::schema::validation::v2_fields::split_lint_findings;
use crate::plan::schema::validation::v2_lints::LintFinding;
use crate::plan::schema::{LoomMetadata, StageDefinition, StageType};

use super::{matching, plan, stage};

const NOT_DISTILLED: &str = "not an ancestor of knowledge-distill stage";
const NOT_VERIFIED: &str = "not an ancestor of any integration-verify stage";

/// A stage of `stage_type` that depends on `deps`.
fn node(id: &str, stage_type: StageType, deps: &[&str]) -> StageDefinition {
    let mut built = stage(id, stage_type, &[]);
    built.dependencies = deps.iter().map(|dep| dep.to_string()).collect();
    built
}

/// bootstrap -> feature -> integration-verify -> knowledge-distill.
fn conforming() -> Vec<StageDefinition> {
    vec![
        node("knowledge-bootstrap", StageType::Knowledge, &[]),
        node("feature", StageType::Standard, &["knowledge-bootstrap"]),
        node(
            "integration-verify",
            StageType::IntegrationVerify,
            &["feature"],
        ),
        node(
            "knowledge-distill",
            StageType::KnowledgeDistill,
            &["integration-verify"],
        ),
    ]
}

/// The stage ids of `found`, which must all be set.
fn stage_ids(found: &[LintFinding]) -> Vec<&str> {
    found
        .iter()
        .map(|finding| finding.stage_id.as_deref().expect("stage finding"))
        .collect()
}

/// Errors under version 2, warnings only under version 1.
fn assert_severity(found: Vec<LintFinding>) {
    let (errors, warnings) = split_lint_findings(2, found.clone());
    assert_eq!(errors.len(), found.len());
    assert!(warnings.is_empty(), "{warnings:?}");
    let (errors, warnings) = split_lint_findings(1, found.clone());
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(warnings.len(), found.len());
}

fn bookend_findings(metadata: &LoomMetadata) -> usize {
    ["ancestor of", "knowledge-distill stages", "has dependents"]
        .iter()
        .map(|needle| matching(metadata, None, needle).len())
        .sum()
}

#[test]
fn distill_lint_flags_a_stage_outside_its_ancestry() {
    let mut stages = conforming();
    stages.push(node("notes", StageType::Knowledge, &[]));
    let found = matching(&plan(stages), None, NOT_DISTILLED);

    assert_eq!(stage_ids(&found), ["notes"]);
    assert_severity(found);
}

#[test]
fn distill_lint_counts_and_dependents() {
    let mut two = conforming();
    two.push(node(
        "knowledge-distill-2",
        StageType::KnowledgeDistill,
        &["knowledge-distill"],
    ));
    let needle = "knowledge-distill stages; a plan needs exactly one";
    let found = matching(&plan(two), None, needle);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].stage_id, None);
    assert!(found[0].error_in_v2);

    let mut dependent = conforming();
    dependent.push(node("after", StageType::Standard, &["knowledge-distill"]));
    let found = matching(&plan(dependent), None, "has dependents");
    assert_eq!(stage_ids(&found), ["knowledge-distill"]);
    assert!(found[0].error_in_v2);
}

#[test]
fn distill_lint_flags_a_plan_with_no_distill_stage() {
    let mut stages = conforming();
    stages.retain(|stage| stage.stage_type != Some(StageType::KnowledgeDistill));
    let found = matching(&plan(stages), None, "knowledge-distill stages");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].stage_id, None);
    assert_eq!(
        found[0].message,
        "the plan has 0 knowledge-distill stages; a plan needs exactly one, after every other stage, so the memory of every stage is curated once"
    );
    assert!(found[0].error_in_v2);
    assert_severity(found);
}

#[test]
fn iv_lint_flags_a_standard_stage_no_verify_stage_follows() {
    let stages = vec![
        node("knowledge-bootstrap", StageType::Knowledge, &[]),
        node("parser", StageType::Standard, &["knowledge-bootstrap"]),
        node("greeting", StageType::Standard, &["parser"]),
        node(
            "integration-verify",
            StageType::IntegrationVerify,
            &["greeting"],
        ),
        node("farewell", StageType::Standard, &["knowledge-bootstrap"]),
        node(
            "knowledge-distill",
            StageType::KnowledgeDistill,
            &["integration-verify", "farewell"],
        ),
    ];
    let found = matching(&plan(stages), None, NOT_VERIFIED);

    assert_eq!(stage_ids(&found), ["farewell"]);
    assert_severity(found);
}

#[test]
fn a_conforming_plan_has_no_bookend_findings() {
    assert_eq!(bookend_findings(&plan(conforming())), 0);
}

#[test]
fn an_omitted_stage_type_is_resolved_by_id() {
    let mut stages = conforming();
    stages[2].stage_type = None;
    stages[3].stage_type = None;

    assert_eq!(bookend_findings(&plan(stages)), 0);
}
