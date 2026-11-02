use std::fs;

use super::evaluate_dir;
use super::tests::{corpus, rust_labels};

#[test]
fn declarations_with_the_same_scope_require_each_labelled_site() {
    let labels = "dialect: rust\ndeclarations:\n  - {path: src/lib.rs, kind: function, scope: [f], line: 1}\n  - {path: src/lib.rs, kind: function, scope: [f], line: 3}\n";
    let dir = corpus(&[
        ("labels.yaml", labels),
        ("src/lib.rs", "fn f() {}\n\nfn f() {}\n"),
    ]);
    let both = evaluate_dir(dir.path()).expect("evaluate both");
    assert_eq!(both.declaration_recall, 1.0, "{both:#?}");
    fs::write(dir.path().join("src/lib.rs"), "fn f() {}\n\n\n").unwrap();
    let one = evaluate_dir(dir.path()).expect("evaluate one");
    assert_eq!(one.declaration_recall, 0.5, "{one:#?}");
    assert!(
        one.failures
            .iter()
            .any(|f| f.contains("src/lib.rs:3 declaration")),
        "{one:#?}"
    );
}

#[test]
fn declaration_labels_cannot_reuse_one_node_at_different_lines() {
    let labels = "dialect: rust\ndeclarations:\n  - {path: src/lib.rs, kind: function, scope: [f], line: 1}\n  - {path: src/lib.rs, kind: function, scope: [f], line: 2}\n";
    let dir = corpus(&[("labels.yaml", labels), ("src/lib.rs", "fn f() {\n}\n")]);
    let error = evaluate_dir(dir.path()).expect_err("one site cannot count twice");
    assert!(
        format!("{error:#}").contains("duplicates a declaration site"),
        "{error:#}"
    );
}

#[test]
fn duplicate_scoring_keys_are_rejected_before_evaluation() {
    for duplicate in [
        "declarations:\n  - {path: a.rs, kind: function, scope: [f], line: 1}\n  - {path: a.rs, kind: function, scope: [f], line: 1}\n",
        "references:\n  - {path: a.rs, line: 1, symbol: f, expect: {external: true}}\n  - {path: a.rs, line: 1, symbol: f, expect: {target: x}}\n",
        "impact:\n  - {start: x, depth: 1, expect: [y]}\n  - {start: x, depth: 1, expect: [z]}\n",
        "syntax_error_files: [a.rs, a.rs]\n",
    ] {
        let labels = format!("dialect: rust\n{duplicate}");
        let dir = corpus(&[("labels.yaml", &labels)]);
        let error = evaluate_dir(dir.path()).expect_err("duplicate must fail");
        assert!(format!("{error:#}").contains("duplicate"), "{error:#}");
    }
}

#[test]
fn multiple_same_line_sites_cannot_select_the_expected_target() {
    let labels = rust_labels(
        "  - {path: src/lib.rs, line: 3, symbol: f, expect: {target: \"src/lib.rs#function:f\"}}\n",
    );
    let source = "fn f() {}\nfn run() {\n    f(); f();\n}\n";
    let dir = corpus(&[("labels.yaml", &labels), ("src/lib.rs", source)]);
    let error = evaluate_dir(dir.path()).expect_err("multiple sites must fail");
    assert!(
        format!("{error:#}").contains("multiple reference sites"),
        "{error:#}"
    );
    let first = source.find("f();").unwrap();
    let selected = labels.replace("symbol: f,", &format!("symbol: f, site_start: {first},"));
    fs::write(dir.path().join("labels.yaml"), selected).unwrap();
    let report = evaluate_dir(dir.path()).expect("explicit site");
    assert_eq!(report.target_recall, 1.0, "{report:#?}");
}

#[test]
fn same_named_calls_on_different_receivers_do_not_select_a_favorable_binding() {
    let source = "struct A; struct B;\nimpl A { fn f(&self) {} }\nimpl B { fn f(&self) {} }\nfn run(a: A, b: B) { a.f(); b.f(); }\n";
    for target in ["src/lib.rs#method:A::f", "src/lib.rs#method:B::f"] {
        let labels = rust_labels(&format!(
            "  - {{path: src/lib.rs, line: 4, symbol: f, expect: {{target: \"{target}\"}}}}\n"
        ));
        let dir = corpus(&[("labels.yaml", &labels), ("src/lib.rs", source)]);
        let error = evaluate_dir(dir.path()).expect_err("multiple receivers must fail");
        assert!(
            format!("{error:#}").contains("multiple reference sites"),
            "{error:#}"
        );
    }
}

#[test]
fn different_labels_cannot_score_the_same_observation_twice() {
    let source = "fn f() {}\nfn run() { f(); }\n";
    let start = source.find("f();").unwrap();
    let labels = rust_labels(&format!(
        "  - {{path: src/lib.rs, line: 2, symbol: f, expect: {{external: true}}}}\n  - {{path: src/lib.rs, line: 2, symbol: f, site_start: {start}, expect: {{external: true}}}}\n"
    ));
    let dir = corpus(&[("labels.yaml", &labels), ("src/lib.rs", source)]);
    let error = evaluate_dir(dir.path()).expect_err("duplicate site must fail");
    assert!(
        format!("{error:#}").contains("duplicates a reference site"),
        "{error:#}"
    );
}
