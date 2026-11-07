//! The definition-shaped wiring lint, and the leaf and coverage rules that
//! `consumer_deferred` and `reachable` satisfy.

use super::*;
use crate::plan::schema::tests::make_stage;
use crate::plan::schema::validation::check_cross_stage_wiring_coverage;
use crate::plan::schema::{
    split_lint_findings, validate, AcceptanceCriterion, LoomConfig, LoomMetadata, ReachableCheck,
    StageDefinition, WiringCheck,
};

#[test]
fn definition_keywords_are_found_after_visibility_and_async() {
    let literal = [
        ("pub(crate) fn declared_paths(", "fn"),
        ("  pub(super) struct Reach", "struct"),
        ("mod eval_tests;", "mod"),
        ("def update_variable_glyph", "def"),
        ("export default function Foo", "function"),
        ("export async function load", "function"),
        ("pub async fn run()", "fn"),
        ("impl Display for Thing", "impl"),
        ("pub trait Source", "trait"),
        ("type Alias = u32", "type"),
        ("class Greeter:", "class"),
        ("export interface Shape", "interface"),
        ("enum Kind {", "enum"),
    ];
    for (pattern, keyword) in literal {
        assert_eq!(
            definition_keyword(pattern, true),
            Some(keyword),
            "{pattern:?}"
        );
    }
    let uses = [
        "fn(",
        "class=",
        "type:",
        "typeof x",
        "functionName(",
        "impl<T>",
        "pub terminals: bool",
        "declared_paths(",
        "default_value",
        "^fn run",
        r"\s*fn run",
        "",
    ];
    for pattern in uses {
        assert_eq!(definition_keyword(pattern, true), None, "{pattern:?}");
    }
}

#[test]
fn regex_patterns_skip_anchors_and_whitespace_tokens() {
    let regex = [
        (r"pub\(crate\) fn declared_paths\(", "fn"),
        (r"^\s*pub\s+fn\s+run", "fn"),
        (r"^pub(crate) fn run", "fn"),
        (r"\s+export\s+default\s+class Foo", "class"),
        (r"pub +struct Config", "struct"),
        (r"^\s*fn\s*\(", "fn"),
        ("mod eval_tests;", "mod"),
    ];
    for (pattern, keyword) in regex {
        assert_eq!(
            definition_keyword(pattern, false),
            Some(keyword),
            "{pattern:?}"
        );
    }
    for pattern in [r"fn\(", r"class=", r"typeof\s+x", r"\bfn run", "run(.*fn"] {
        assert_eq!(definition_keyword(pattern, false), None, "{pattern:?}");
    }
}

fn wired(pattern: &str, literal: bool) -> StageDefinition {
    StageDefinition {
        wiring: vec![WiringCheck {
            source: "src/lib.rs".to_string(),
            pattern: pattern.to_string(),
            description: "the unit is used".to_string(),
            literal,
        }],
        ..make_stage("wired", "Wired")
    }
}

fn findings(stage: StageDefinition, version: u32) -> Vec<LintFinding> {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version,
            stages: vec![stage],
            ..Default::default()
        },
    };
    let ctx = LintContext {
        metadata: &metadata,
        repo_root: None,
    };
    let mut out = Vec::new();
    check_definition_shaped(&ctx, &mut out);
    out
}

#[test]
fn a_definition_shaped_pattern_is_an_error_in_v2_and_a_warning_in_v1() {
    let (errors, warnings) = split_lint_findings(2, findings(wired("pub fn run(", true), 2));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(
        errors[0]
            .message
            .contains("starts with the definition keyword `fn`"),
        "{errors:?}"
    );
    assert!(errors[0].message.contains("Wiring #1"), "{errors:?}");
    assert_eq!(errors[0].stage_id.as_deref(), Some("wired"));

    let (errors, warnings) = split_lint_findings(1, findings(wired(r"pub fn run\(", false), 1));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("definition keyword"), "{warnings:?}");
}

#[test]
fn a_use_site_pattern_and_other_commands_draw_no_finding() {
    assert!(findings(wired("run(", true), 2).is_empty());
    let mut stage = wired("run(", true);
    stage.acceptance = vec![AcceptanceCriterion::Simple(
        "rg -q 'pub fn run' src".to_string(),
    )];
    assert!(
        findings(stage, 2).is_empty(),
        "only wiring entries are linted"
    );
}

#[test]
fn a_long_pattern_is_cut_in_the_message() {
    let pattern = format!("fn {}", "x".repeat(200));
    let out = findings(wired(&pattern, true), 2);
    assert_eq!(out.len(), 1);
    assert!(
        !out[0].message.contains(&"x".repeat(100)),
        "{}",
        out[0].message
    );
    assert!(out[0].message.contains("..."), "{}", out[0].message);
}

const LEAF: &str = "Standard leaf stage has artifacts but no wiring";
const COVERAGE: &str = "artifacts are not covered by wiring checks";

/// A lone standard stage with an artifact: a leaf, and nothing downstream covers it.
fn leaf() -> StageDefinition {
    let mut stage = make_stage("leaf", "Leaf");
    stage.artifacts = vec!["src/greeting.rs".to_string()];
    stage.acceptance = vec![AcceptanceCriterion::Simple("cargo test".to_string())];
    stage
}

fn leaf_errors(stage: StageDefinition) -> bool {
    let metadata = LoomMetadata {
        loom: LoomConfig {
            version: 1,
            stages: vec![stage],
            ..Default::default()
        },
    };
    let errors = validate(&metadata).err().unwrap_or_default();
    errors.iter().any(|error| error.message.contains(LEAF))
}

fn covered(stage: &StageDefinition) -> bool {
    !check_cross_stage_wiring_coverage(std::slice::from_ref(stage))
        .iter()
        .any(|warning| warning.contains(COVERAGE))
}

fn reachable() -> Vec<ReachableCheck> {
    vec![ReachableCheck {
        symbol: "greet".to_string(),
        from: "main".to_string(),
        min_confidence: None,
        description: "greet is called from main".to_string(),
    }]
}

#[test]
fn consumer_deferred_wiring_and_reachable_each_satisfy_the_leaf_rule() {
    assert!(leaf_errors(leaf()), "a bare leaf is an error");
    assert!(!leaf_errors(wired_leaf()), "wiring satisfies it");
    let mut deferred = leaf();
    deferred.consumer_deferred = Some("the CLI that reads it is a later plan".to_string());
    assert!(!leaf_errors(deferred), "consumer_deferred satisfies it");
    let mut reaches = leaf();
    reaches.reachable = reachable();
    assert!(!leaf_errors(reaches), "reachable satisfies it");
}

fn wired_leaf() -> StageDefinition {
    StageDefinition {
        wiring: wired("greet(", true).wiring,
        ..leaf()
    }
}

#[test]
fn consumer_deferred_and_reachable_remove_the_coverage_warning() {
    assert!(!covered(&leaf()), "a bare stage draws the warning");
    assert!(covered(&wired_leaf()));
    let mut deferred = leaf();
    deferred.consumer_deferred = Some("the consumer arrives in a later plan".to_string());
    assert!(covered(&deferred));
    let mut reaches = leaf();
    reaches.reachable = reachable();
    assert!(covered(&reaches));
}
