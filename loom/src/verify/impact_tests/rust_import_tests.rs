//! Package-name imports need manifest evidence before they select a test.

use super::*;

#[test]
fn an_unresolved_package_import_is_a_note_while_bound_tests_run() {
    let integration = ADD_TEST.replace("crate::add", "demo::add");
    let temp = repo_with_base(
        ("Cargo.toml", CARGO_TOML),
        Some(("tests/add_test.rs", &integration)),
    );
    let root = temp.path().canonicalize().unwrap();
    let recorder = Recorder::default();

    let outcome = select_after_changing_add(&root, &recorder);

    let commands = recorder.commands.into_inner();
    assert_eq!(commands.len(), 1, "{commands:?}");
    let (command, _) = &commands[0];
    assert_eq!(
        command,
        "cargo test --lib -- --exact add_tests::adds_two tests::adds_in_unit"
    );
    assert_eq!(outcome.ran, vec![command.clone()]);
    assert_eq!(
        outcome.notes,
        [format!(
            "tests in tests/add_test.rs are not selected: they reach this stage's changes only \
             through an ambiguous call matched by name; {FULL_SUITE}"
        )]
    );
}
