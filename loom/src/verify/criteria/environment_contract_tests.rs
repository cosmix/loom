//! Contract `a-refused-sandbox-is-an-environment-red`: `environment_marker`
//! reads the output text, so a refused sandbox is an environment failure and a
//! failed assertion is not.
use super::environment::environment_marker;

#[test]
fn a_refused_sandbox_is_an_environment_red() {
    let refused = "sandbox-exec: sandbox_apply: Operation not permitted";

    let marker = environment_marker("", refused).expect("a refused sandbox is an environment red");
    assert!(
        refused.to_lowercase().contains(marker),
        "the marker must be text found in the output, got: {marker}"
    );
    assert!(
        environment_marker(refused, "").is_some(),
        "a marker on stdout counts as well"
    );

    let assertion = "thread 'adds' panicked at src/calc.rs:4:5:\n\
                     assertion `left == right` failed\n  left: 1\n right: 2";
    assert_eq!(
        environment_marker("", assertion),
        None,
        "a failed assertion is the test's own failure"
    );
    assert_eq!(environment_marker(assertion, ""), None);
}
