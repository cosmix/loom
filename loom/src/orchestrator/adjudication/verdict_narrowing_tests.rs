use serde_json::{json, Value};

use super::*;

fn narrowing() -> Value {
    json!({
        "field": "acceptance",
        "patches": [{"op": "delete", "index": 0}],
        "leaves_out": [{
            "index": 0, "tests": ["net_fetch"], "runs_at": "ci", "reason": "network",
        }],
    })
}

fn base_run(timed_out: bool) -> BaseEvidence {
    BaseEvidence {
        index: 0,
        command: "cargo test".to_string(),
        base: "b1".to_string(),
        passed: false,
        executed: Some(3),
        failed: Some(1),
        build_failed: false,
        timed_out,
    }
}

/// The refusal of `plan_patch` on criteria `cargo test` and `cargo test
/// --lib`, each with a base run failing one of forty tests.
fn refusal(plan_patch: Value) -> String {
    let current = [json!("cargo test"), json!("cargo test --lib")];
    let evidence = [
        BaseEvidence {
            executed: Some(40),
            ..base_run(false)
        },
        BaseEvidence {
            index: 1,
            command: "cargo test --lib".to_string(),
            executed: Some(40),
            ..base_run(false)
        },
    ];
    match check_narrowing("s1", "1", &plan_patch, &current, &evidence) {
        Ok(handoffs) => panic!("the narrowing must be refused, got {handoffs:?}"),
        Err(text) => text,
    }
}

/// A delete of criterion 0 with `leaves_out`.
fn deleting(leaves_out: Value) -> Value {
    json!({
        "field": "acceptance",
        "patches": [{"op": "delete", "index": 0}],
        "leaves_out": leaves_out,
    })
}

fn entry(index: usize, tests: &[&str], runs_at: &str) -> Value {
    json!({"index": index, "tests": tests, "runs_at": runs_at, "reason": "network"})
}

#[test]
fn a_timed_out_base_run_hands_off_unverified() {
    let current = [json!("cargo test")];
    let check = |timed_out| {
        check_narrowing("s1", "1", &narrowing(), &current, &[base_run(timed_out)]).unwrap()
    };
    assert!(check(false)[0].verified);
    assert!(!check(true)[0].verified);
}

#[test]
fn a_bare_command_and_its_command_object_are_the_same_criterion() {
    let entry = json!("cargo test");
    assert!(same_entry(&json!({"command": "cargo test"}), &entry));
    assert!(same_entry(&json!("command: cargo test"), &entry));
    assert!(same_entry(
        &json!("cargo test"),
        &json!({"command": "cargo test"})
    ));
    assert!(!same_entry(
        &json!({"command": "cargo test", "timeout_secs": 60}),
        &entry
    ));
    assert!(!same_entry(&json!("cargo test --lib"), &entry));
}

#[test]
fn a_replace_restating_the_criterion_in_yaml_changes_nothing() {
    let patch = json!({
        "field": "acceptance",
        "patches": [{"op": "replace", "index": 0, "value": "command: cargo test"}],
        "leaves_out": [entry(0, &["net_fetch"], "ci")],
    });
    let text = refusal(patch);
    assert!(text.contains("changes nothing"), "{text}");
}

#[test]
fn leaving_out_more_tests_than_the_base_run_failed_is_refused() {
    let text = refusal(deleting(json!([entry(0, &["a", "b"], "ci")])));
    assert!(
        text.contains("fails only 1 test(s)") && text.contains("leaves out 2"),
        "{text}"
    );
}

#[test]
fn a_leaves_out_entry_no_patch_amends_is_refused() {
    let text = refusal(deleting(json!([
        entry(0, &["a"], "ci"),
        entry(1, &["b"], "ci"),
    ])));
    assert!(
        text.contains("names criterion 1, which no replace or delete patch amends"),
        "{text}"
    );
}

#[test]
fn a_criterion_named_twice_in_leaves_out_is_refused() {
    let text = refusal(deleting(json!([
        entry(0, &["a"], "ci"),
        entry(0, &["b"], "ci"),
    ])));
    assert!(text.contains("names criterion 0 twice"), "{text}");
}

#[test]
fn a_leaves_out_entry_naming_no_test_is_refused() {
    for tests in [&[][..], &[" "][..]] {
        let text = refusal(deleting(json!([entry(0, tests, "ci")])));
        assert!(text.contains("names no test"), "{tests:?}: {text}");
    }
}

#[test]
fn a_leaves_out_entry_with_an_empty_runs_at_is_refused() {
    let text = refusal(deleting(json!([entry(0, &["a"], " ")])));
    assert!(text.contains("empty runs_at"), "{text}");
}

#[test]
fn only_a_leaves_out_entry_reads_base_runs() {
    let replace = json!({"field": "acceptance", "patch": {"op": "replace", "index": 0}});
    assert!(!names_left_out(&replace));
    let mut empty = replace.clone();
    empty["leaves_out"] = json!([]);
    assert!(!names_left_out(&empty));
    assert!(names_left_out(&deleting(json!([entry(0, &["t"], "ci")]))));
    let mut malformed = replace;
    malformed["leaves_out"] = json!("not a list");
    assert!(
        names_left_out(&malformed),
        "a malformed entry must still be parsed"
    );
}
