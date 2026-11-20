//! Contract `a-changed-assertion-is-listed`: `lost_statements` compares the
//! assertion statements themselves, so an assertion weakened in place is listed
//! even though the count of assertions stays the same.
use super::lost_statements;

#[test]
fn a_changed_assertion_is_listed() {
    let before = "#[test]\nfn checks() {\n    assert_eq!(a, 1);\n    assert!(ok);\n}\n";
    let after = "#[test]\nfn checks() {\n    assert_eq!(a, 2);\n    assert!(ok);\n}\n";

    let lost = lost_statements("src/calc_tests.rs", before, after);

    assert_eq!(
        lost.len(),
        1,
        "exactly the changed assertion, got: {lost:?}"
    );
    assert!(
        lost[0].contains("assert_eq!(a, 1)"),
        "the listed statement must be the frozen one, got: {lost:?}"
    );
    assert!(
        lost_statements("notes.unknownext", before, after).is_empty(),
        "an extension without a language profile lists nothing"
    );
}
