//! Tests for `commands/knowledge/eval.rs`.
//!
//! The scoring/parsing/gating functions are tested as pure functions over
//! constructed item-id lists and cases — never against the live index, which
//! `eval()` itself reads and which is what `loom knowledge eval` (run by a
//! human or a plan) exists to score.

use super::*;

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn case(name: &str, query: &str, expect: &[&str], forbid: &[&str]) -> EvalCase {
    EvalCase {
        name: name.to_string(),
        query: query.to_string(),
        mode: EvalMode::Prompt,
        budget_tokens: None,
        stage_fields: Vec::new(),
        require_ids: Vec::new(),
        expect: ids(expect),
        forbid: ids(forbid),
    }
}

#[test]
fn parses_a_valid_cases_file() {
    let yaml = r#"
pass_floor: 0.6
cases:
  - name: alpha
    query: "find the widget"
    expect: ["a.md#b#0"]
  - name: beta
    query: "stage query"
    mode: stage
    stage_fields: ["src/foo.rs"]
    forbid: ["noisy#id"]
"#;
    let parsed: CasesFile = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(parsed.pass_floor, 0.6);
    assert_eq!(parsed.cases.len(), 2);
    assert_eq!(parsed.cases[0].name, "alpha");
    assert_eq!(parsed.cases[0].expect, ids(&["a.md#b#0"]));
    assert_eq!(parsed.cases[1].mode, EvalMode::Stage);
    assert_eq!(parsed.cases[1].stage_fields, ids(&["src/foo.rs"]));
    validate_cases(&parsed).unwrap();
}

#[test]
fn default_pass_floor_applies_when_absent() {
    let yaml = "cases:\n  - name: a\n    query: q\n    expect: [\"x\"]\n";
    let parsed: CasesFile = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(parsed.pass_floor, 0.5);
}

#[test]
fn validate_cases_rejects_a_duplicate_name() {
    let file = CasesFile {
        pass_floor: 0.5,
        cases: vec![
            case("dup", "q1", &["x"], &[]),
            case("dup", "q2", &["y"], &[]),
        ],
    };
    let error = validate_cases(&file).unwrap_err();
    assert!(error.to_string().contains("dup"));
}

#[test]
fn validate_cases_rejects_an_empty_query() {
    let file = CasesFile {
        pass_floor: 0.5,
        cases: vec![case("blank", "   ", &["x"], &[])],
    };
    let error = validate_cases(&file).unwrap_err();
    assert!(error.to_string().contains("blank"));
}

#[test]
fn validate_cases_rejects_neither_expect_nor_forbid() {
    let file = CasesFile {
        pass_floor: 0.5,
        cases: vec![case("toothless", "q", &[], &[])],
    };
    let error = validate_cases(&file).unwrap_err();
    assert!(error.to_string().contains("toothless"));
}

#[test]
fn hit_at_5_only_looks_at_the_first_five_items() {
    let expect = ids(&["target"]);
    let too_far = ids(&["a", "b", "c", "d", "e", "target"]);
    assert!(!hit_at_5(&expect, &too_far));
    let within_reach = ids(&["a", "b", "target", "d", "e", "f"]);
    assert!(hit_at_5(&expect, &within_reach));
}

#[test]
fn mrr_is_the_reciprocal_of_the_first_matching_rank() {
    let expect = ids(&["target"]);
    let items = ids(&["a", "target", "b"]);
    assert_eq!(mrr(&expect, &items), 0.5);
}

#[test]
fn mrr_is_zero_when_expect_never_appears() {
    let expect = ids(&["target"]);
    let items = ids(&["a", "b"]);
    assert_eq!(mrr(&expect, &items), 0.0);
}

#[test]
fn forbid_violations_lists_every_forbidden_id_present() {
    let forbid = ids(&["bad1", "bad2", "absent"]);
    let items = ids(&["bad1", "ok", "bad2"]);
    assert_eq!(forbid_violations(&forbid, &items), ids(&["bad1", "bad2"]));
}

#[test]
fn aggregate_excludes_forbid_only_cases_from_precision() {
    let results = vec![
        CaseResult {
            name: "a".to_string(),
            counts_toward_precision: true,
            hit_at_5: true,
            mrr: 1.0,
            forbid_violations: Vec::new(),
        },
        CaseResult {
            name: "b".to_string(),
            counts_toward_precision: false,
            hit_at_5: false,
            mrr: 0.0,
            forbid_violations: ids(&["x"]),
        },
    ];
    let aggregates = aggregate(&results);
    assert_eq!(aggregates.precision_applicable, 1);
    assert_eq!(aggregates.precision_at_5, 1.0);
    assert_eq!(aggregates.forbid_violations, 1);
}

#[test]
fn exit_reason_is_none_when_precision_clears_the_floor_and_nothing_is_forbidden() {
    let aggregates = Aggregates {
        precision_at_5: 0.75,
        precision_hits: 3,
        precision_applicable: 4,
        mean_mrr: 0.5,
        forbid_violations: 0,
    };
    assert!(exit_reason(&aggregates, 0.5).is_none());
}

#[test]
fn exit_reason_names_a_low_precision_score() {
    let aggregates = Aggregates {
        precision_at_5: 0.2,
        precision_hits: 1,
        precision_applicable: 5,
        mean_mrr: 0.1,
        forbid_violations: 0,
    };
    let reason = exit_reason(&aggregates, 0.5).unwrap();
    assert!(reason.contains("precision@5"));
}

#[test]
fn exit_reason_names_a_forbid_violation_even_with_perfect_precision() {
    let aggregates = Aggregates {
        precision_at_5: 1.0,
        precision_hits: 2,
        precision_applicable: 2,
        mean_mrr: 1.0,
        forbid_violations: 3,
    };
    let reason = exit_reason(&aggregates, 0.5).unwrap();
    assert!(reason.contains("forbid violation"));
}

#[test]
fn build_query_text_ignores_stage_fields_in_prompt_mode() {
    let mut prompt_case = case("p", "just the prompt", &["x"], &[]);
    prompt_case.stage_fields = ids(&["ignored"]);
    assert_eq!(build_query_text(&prompt_case), "just the prompt");
}

#[test]
fn build_query_text_folds_stage_fields_in_stage_mode() {
    let mut stage_case = case("s", "the query", &["x"], &[]);
    stage_case.mode = EvalMode::Stage;
    stage_case.stage_fields = ids(&["src/a.rs", "src/b.rs"]);
    assert_eq!(
        build_query_text(&stage_case),
        "the query\nsrc/a.rs\nsrc/b.rs"
    );
}

#[test]
fn resolve_budget_prefers_cli_then_case_then_mode_default() {
    let mut prompt_case = case("b", "q", &["x"], &[]);
    assert_eq!(
        resolve_budget(&prompt_case, None),
        DEFAULT_PROMPT_BUDGET_TOKENS
    );
    prompt_case.budget_tokens = Some(999);
    assert_eq!(resolve_budget(&prompt_case, None), 999);
    assert_eq!(resolve_budget(&prompt_case, Some(42)), 42);
}

#[test]
fn resolve_budget_defaults_stage_mode_higher_than_prompt_mode() {
    let mut stage_case = case("s", "q", &["x"], &[]);
    stage_case.mode = EvalMode::Stage;
    assert_eq!(
        resolve_budget(&stage_case, None),
        DEFAULT_STAGE_BUDGET_TOKENS
    );
}
