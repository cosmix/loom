//! `TI-edit-<path>` compares assertion statements: reflows and comment edits
//! raise nothing, a changed or deleted statement raises.

use super::tests::{ids, sha256_hex, Repo};
use super::*;

const TESTS_FILE: &str = "src/calc_tests.rs";
const PYTHON_FILE: &str = "tests/test_calc.py";
const EDIT_ID: &str = "TI-edit-src/calc_tests.rs";

/// The integrity events after `base` is committed at `path` and `edited`
/// replaces it in the worktree.
fn events_after(path: &str, base: &str, edited: &str) -> Vec<IntegrityEvent> {
    let repo = Repo::new(&[(path, base)]);
    repo.write(path, edited);
    repo.events(&[])
}

fn edit(events: &[IntegrityEvent]) -> &IntegrityEvent {
    events
        .iter()
        .find(|event| event.kind == EventKind::AssertionEdit)
        .unwrap_or_else(|| panic!("no edit event in {:?}", ids(events)))
}

#[test]
fn reflow_beside_a_real_edit_in_the_same_hunk_raises_nothing() {
    let base = "#[test]\nfn adds() {\n    let a = 1;\n    assert_eq!(compute(1, 2), 3);\n}\n";
    let edited = "#[test]\nfn adds() {\n    let a = 2;\n    assert_eq!(\n        compute(1, 2),\n        3,\n    );\n}\n";

    assert_eq!(events_after(TESTS_FILE, base, edited), Vec::new());
}

#[test]
fn changed_continuation_line_raises_with_the_base_statement() {
    let base =
        "#[test]\nfn adds() {\n    assert_eq!(\n        compute(1, 2),\n        3,\n    );\n}\n";
    let edited = base.replace("        3,\n", "        4,\n");

    let events = events_after(TESTS_FILE, base, &edited);

    assert_eq!(ids(&events), [EDIT_ID]);
    let detail = &events[0].detail;
    assert_eq!(detail.len(), 1, "{detail:?}");
    assert!(
        detail[0].contains("compute(1, 2), 3,") && !detail[0].contains('4'),
        "{detail:?}"
    );
}

#[test]
fn comment_only_edit_beside_an_assertion_raises_nothing() {
    let base =
        "#[test]\nfn adds() {\n    // adds small numbers\n    assert_eq!(compute(1, 2), 3);\n}\n";
    let edited = base.replace("adds small numbers", "adds two numbers, for example");

    assert_eq!(events_after(TESTS_FILE, base, &edited), Vec::new());
}

#[test]
fn statement_moved_within_its_file_raises_nothing() {
    let base = "#[test]\nfn adds() {\n    assert_eq!(compute(1, 2), 3);\n    let a = 1;\n    \
                assert!(a > 0);\n}\n";
    let swapped = "#[test]\nfn adds() {\n    let a = 1;\n    assert!(a > 0);\n    \
                   assert_eq!(compute(1, 2), 3);\n}\n";

    assert_eq!(events_after(TESTS_FILE, base, swapped), Vec::new());
}

#[test]
fn deleted_statement_raises_with_its_text() {
    let base = "#[test]\nfn adds() {\n    assert_eq!(compute(1, 2), 3);\n    assert!(true);\n}\n";
    let edited = "#[test]\nfn adds() {\n    assert_eq!(compute(1, 2), 3);\n}\n";

    let events = events_after(TESTS_FILE, base, edited);

    assert_eq!(edit(&events).id, EDIT_ID);
    assert_eq!(edit(&events).detail, ["assert!(true);"]);
}

#[test]
fn python_assert_reflowed_over_two_lines_raises_nothing() {
    let base = "def test_x():\n    x = (1, 2)\n    assert x == (1, 2)\n";
    let edited = "def test_x():\n    x = (1, 2)\n    assert x == (1,\n                  2)\n";

    assert_eq!(events_after(PYTHON_FILE, base, edited), Vec::new());
}

#[test]
fn deleted_test_file_raises_with_every_statement() {
    let base = "#[test]\nfn adds() {\n    assert_eq!(compute(1, 2), 3);\n    assert!(true);\n}\n";
    let repo = Repo::new(&[(TESTS_FILE, base)]);
    std::fs::remove_file(repo.root.join(TESTS_FILE)).unwrap();

    let events = repo.events(&[]);

    let deleted = edit(&events);
    assert_eq!(deleted.id, EDIT_ID);
    assert_eq!(deleted.current_sha256, None);
    assert_eq!(
        deleted.detail,
        ["assert_eq!(compute(1, 2), 3);", "assert!(true);"]
    );
}

#[test]
fn long_statement_detail_is_cut_at_600_bytes() {
    let long = "x".repeat(900);
    let base = format!("#[test]\nfn adds() {{\n    assert!(f(\"{long}\"));\n}}\n");
    let edited = "#[test]\nfn adds() {}\n";

    let events = events_after(TESTS_FILE, &base, edited);

    assert_eq!(edit(&events).detail.len(), 1);
    assert_eq!(edit(&events).detail[0].len(), 600);
}

fn accepted_at(event: &IntegrityEvent) -> Accepted {
    Accepted {
        accepted: vec![AcceptedEvent {
            event: event.id.clone(),
            kind: EventKind::AssertionEdit,
            language: event.language.clone(),
            path: event.path.clone(),
            base: None,
            accepted_current: None,
            accepted_sha256: event.current_sha256.clone(),
            dispute: 1,
        }],
        ..Accepted::default()
    }
}

#[test]
fn accepted_edit_stops_covering_the_file_once_a_second_assert_is_lost() {
    let base = "#[test]\nfn adds() {\n    assert!(one());\n    assert!(two());\n    assert!(three());\n}\n";
    let first = base.replace("    assert!(one());\n", "");
    let second = first.replace("    assert!(two());\n", "");
    let repo = Repo::new(&[(TESTS_FILE, base)]);

    repo.write(TESTS_FILE, &first);
    let events = repo.events(&[]);
    let accepted = accepted_at(edit(&events));
    assert_eq!(edit(&events).current_sha256, Some(sha256_hex(&first)));
    assert_eq!(shortfall(edit(&events), &accepted), None);

    repo.write(TESTS_FILE, &second);
    let events = repo.events(&[]);

    assert!(shortfall(edit(&events), &accepted).is_some());
}

#[test]
fn kotlin_infix_after_a_multi_line_block_raises_when_the_expectation_changes() {
    let path = "src/test/kotlin/CalcTest.kt";
    let base =
        "fun doubles() {\n    listOf(1, 2).map {\n        it * 2\n    } shouldBe listOf(2, 4)\n}\n";
    let edited = base.replace("(2, 4)", "(2, 5)");

    let events = events_after(path, base, &edited);

    assert_eq!(ids(&events), ["TI-edit-src/test/kotlin/CalcTest.kt"]);
    assert!(
        events[0].detail[0].contains("(2, 4)"),
        "{:?}",
        events[0].detail
    );
}
