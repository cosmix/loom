//! Contracts for the criteria-authoring stage: the plan lints. A wiring
//! pattern shaped like a definition is refused in a version 2 plan and
//! warned about in version 1; `consumer_deferred` is a bounded one-line
//! reason that satisfies the leaf rule and the coverage warning, and
//! `reachable` satisfies the coverage warning too.

#[path = "criteria_authoring_contracts/plans.rs"]
mod plans;
#[path = "criteria_authoring_contracts/support.rs"]
mod support;

use loom::commands::plan::verify::report::VerifyReport;
use loom::models::stage::{PlanIdentity, Stage};
use loom::plan::schema::StageDefinition;
use plans::{consumer_deferred, standard, v1_plan, v2_plan, wiring, ARTIFACTS, REACHABLE};
use support::{all_warnings, verify};

const DEFINITION: &str = "starts with the definition keyword";
const LEAF: &str = "Standard leaf stage has artifacts but no wiring";
const COVERAGE: &str = "artifacts are not covered by wiring checks";

fn error_messages(report: &VerifyReport) -> Vec<String> {
    report.errors.iter().map(|e| e.message.clone()).collect()
}

fn mentions(items: &[String], needle: &str) -> bool {
    items.iter().any(|item| item.contains(needle))
}

#[test]
fn definition_shaped_wiring_is_rejected() {
    let refused = [
        "pub(crate) fn declared_paths(",
        "mod eval_tests;",
        "def update_variable_glyph",
        "export default function Foo",
    ];
    for pattern in refused {
        let report = verify(&v2_plan("", &wiring(pattern, true)));
        assert!(!report.stopped, "{pattern:?}: the plan parses");
        let errors = error_messages(&report);
        assert!(
            mentions(&errors, DEFINITION),
            "{pattern:?} is an error: {errors:?}"
        );
    }
    for pattern in [
        "pub terminals: bool",
        "declared_paths(",
        "class=",
        "typeof x",
    ] {
        let report = verify(&v2_plan("", &wiring(pattern, true)));
        assert!(!report.stopped, "{pattern:?}: the plan parses");
        let errors = error_messages(&report);
        assert!(
            errors.is_empty(),
            "{pattern:?} is a use, not a definition: {errors:?}"
        );
        let warnings = all_warnings(&report);
        assert!(
            !mentions(&warnings, DEFINITION),
            "{pattern:?}: {warnings:?}"
        );
    }
    definition_shapes_warn_in_version_1();
}

/// Version 1 plans, whose patterns are regexes, draw the warning only.
fn definition_shapes_warn_in_version_1() {
    let regex_patterns = [
        r"pub\(crate\) fn declared_paths\(",
        "mod eval_tests;",
        "def update_variable_glyph",
    ];
    for pattern in regex_patterns {
        let stage = standard("wired", &["true"], &wiring(pattern, false));
        let report = verify(&v1_plan(&stage));
        assert!(!report.stopped, "{pattern:?}: the version 1 plan parses");
        let errors = error_messages(&report);
        assert!(
            !mentions(&errors, DEFINITION),
            "{pattern:?} warns in version 1: {errors:?}"
        );
        let structural = &report.warnings.structural;
        assert!(
            mentions(structural, DEFINITION),
            "{pattern:?}: {structural:?}"
        );
    }
}

/// A version 1 plan whose one standard stage has artifacts and `extra`.
fn leaf_plan(extra: &str) -> VerifyReport {
    let keys = format!("{ARTIFACTS}{extra}");
    verify(&v1_plan(&standard(
        "leaf",
        &["cargo test --test greeting"],
        &keys,
    )))
}

#[test]
fn consumer_deferred_satisfies_the_leaf_rule() {
    let bare = leaf_plan("");
    assert!(
        mentions(&error_messages(&bare), LEAF),
        "{:?}",
        error_messages(&bare)
    );

    let deferred = leaf_plan(&consumer_deferred("the CLI that reads it is a later plan"));
    assert!(
        !deferred.stopped,
        "consumer_deferred parses: {:?}",
        error_messages(&deferred)
    );
    assert!(
        error_messages(&deferred).is_empty(),
        "{:?}",
        error_messages(&deferred)
    );
    assert!(deferred.valid);

    let wired = leaf_plan(&wiring("mod greeting", false));
    assert!(
        error_messages(&wired).is_empty(),
        "{:?}",
        error_messages(&wired)
    );
}

#[test]
fn consumer_deferred_text_is_bounded() {
    let refused = [
        ("empty", String::new()),
        ("blank", "   ".to_string()),
        ("301 bytes", "a".repeat(301)),
        ("a tab", "a\tb".to_string()),
        ("an embedded newline", "a\nb".to_string()),
    ];
    for (label, reason) in refused {
        let report = leaf_plan(&consumer_deferred(&reason));
        assert!(report.stopped, "{label}: the plan must fail to parse");
        assert!(!report.valid, "{label}: the plan is not valid");
    }
    let exact = "a".repeat(300);
    let report = leaf_plan(&consumer_deferred(&exact));
    assert!(
        !report.stopped,
        "300 bytes parse: {:?}",
        error_messages(&report)
    );
    assert!(report.valid, "{:?}", error_messages(&report));

    let definition = StageDefinition {
        id: "leaf".to_string(),
        name: "leaf".to_string(),
        consumer_deferred: Some(exact.clone()),
        ..Default::default()
    };
    let identity = PlanIdentity {
        id: "plan",
        version: 1,
        ratchet_files: &[],
    };
    let stage = Stage::from_definition(&definition, &identity);
    assert_eq!(stage.consumer_deferred.as_deref(), Some(exact.as_str()));
}

#[test]
fn reachable_satisfies_the_coverage_warning() {
    let coverage_warned = |extra: &str| {
        let report = verify(&v2_plan("", &format!("{ARTIFACTS}{extra}")));
        assert!(!report.stopped, "{extra:?}: {:?}", error_messages(&report));
        assert!(
            error_messages(&report).is_empty(),
            "{:?}",
            error_messages(&report)
        );
        mentions(&all_warnings(&report), COVERAGE)
    };
    assert!(
        !coverage_warned(REACHABLE),
        "reachable proves the entry-point reach"
    );
    let deferred = consumer_deferred("the consumer is planned for a later plan");
    assert!(
        !coverage_warned(&deferred),
        "a deferred consumer needs no wiring check"
    );
    assert!(
        coverage_warned(""),
        "a stage with neither still draws the warning"
    );
}
