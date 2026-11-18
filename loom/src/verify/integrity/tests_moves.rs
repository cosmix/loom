//! Cross-file moves through a whole scan: a moved test cancels only when the
//! runner still collects it and it runs identically, and a cancelling partner
//! joins the accepted pin.

use super::tests::{ids, sha256_hex, Repo};
use super::*;

const CARGO: &str = "[package]\nname = \"calc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LIB_ONE: &str = "#[cfg(test)]\nmod calc_tests;\n";
const LIB_BOTH: &str = "#[cfg(test)]\nmod calc_tests;\n#[cfg(test)]\nmod calc_more_tests;\n";
const TESTS: &str = "src/calc_tests.rs";
const MORE: &str = "src/calc_more_tests.rs";
const EDIT_ID: &str = "TI-edit-src/calc_tests.rs";
const ADDS: &str = "#[test]\nfn adds() {\n    assert_eq!(compute(1, 2), 3);\n}\n";
const SUBTRACTS: &str =
    "#[test]\nfn subtracts() {\n    let a = 1;\n    assert_eq!(compute(a, -1), 0);\n}\n";
const DOUBLES: &str = "#[test]\nfn doubles() {\n    assert_eq!(compute(2, 2), 4);\n}\n";

fn rust_file(fns: &[&str]) -> String {
    format!("use crate::compute;\n\n{}", fns.join("\n"))
}

/// A crate whose base test file holds `fns`, with the files in `extra`.
fn crate_repo(fns: &[&str], extra: &[(&str, &str)]) -> Repo {
    let tests = rust_file(fns);
    let mut files = vec![
        ("Cargo.toml", CARGO),
        ("src/lib.rs", LIB_ONE),
        (TESTS, tests.as_str()),
    ];
    files.extend_from_slice(extra);
    Repo::new(&files)
}

/// Rust test source moved: `kept` stay in the base file, `moved` go to MORE
/// (declared by lib.rs).
fn move_to_more(repo: &Repo, kept: &[&str], moved: &[&str]) -> Vec<IntegrityEvent> {
    repo.write(TESTS, &rust_file(kept));
    repo.write(MORE, &rust_file(moved));
    repo.write("src/lib.rs", LIB_BOTH);
    repo.events(&[])
}

fn edit_ids(events: &[IntegrityEvent]) -> Vec<&str> {
    events
        .iter()
        .filter(|event| event.kind == EventKind::AssertionEdit)
        .map(|event| event.id.as_str())
        .collect()
}

#[test]
fn identical_move_into_a_second_test_file_raises_nothing() {
    let repo = crate_repo(&[ADDS, SUBTRACTS], &[]);

    assert_eq!(move_to_more(&repo, &[ADDS], &[SUBTRACTS]), Vec::new());
}

#[test]
fn changed_body_line_with_an_identical_assert_raises_on_the_source() {
    let repo = crate_repo(&[ADDS, SUBTRACTS], &[]);
    let edited = SUBTRACTS.replace("let a = 1;", "let a = 2;");

    let events = move_to_more(&repo, &[ADDS], &[&edited]);

    assert_eq!(edit_ids(&events), [EDIT_ID]);
}

#[test]
fn moved_rust_test_gaining_ignore_raises() {
    let repo = crate_repo(&[ADDS, SUBTRACTS], &[]);
    let ignored = format!("#[ignore]\n{SUBTRACTS}");

    assert_eq!(
        edit_ids(&move_to_more(&repo, &[ADDS], &[&ignored])),
        [EDIT_ID]
    );
}

#[test]
fn multi_line_cfg_attr_moved_verbatim_cancels_and_a_gained_ignore_raises() {
    let attributed = "#[test]\n#[cfg_attr(\n    feature = \"slow\",\n    allow(unused)\n)]\nfn subtracts() {\n    assert_eq!(compute(1, -1), 0);\n}\n";
    let repo = crate_repo(&[ADDS, attributed], &[]);
    assert_eq!(move_to_more(&repo, &[ADDS], &[attributed]), Vec::new());

    let repo = crate_repo(&[ADDS, attributed], &[]);
    let ignored = format!("#[ignore]\n{attributed}");
    assert_eq!(
        edit_ids(&move_to_more(&repo, &[ADDS], &[&ignored])),
        [EDIT_ID]
    );
}

#[test]
fn move_into_a_file_no_mod_line_reaches_raises() {
    let repo = crate_repo(&[ADDS, SUBTRACTS], &[]);
    repo.write(TESTS, &rust_file(&[ADDS]));
    repo.write(MORE, &rust_file(&[SUBTRACTS]));

    assert_eq!(edit_ids(&repo.events(&[])), [EDIT_ID]);
}

const PY_ADD: &str = "def test_add():\n    assert add(1, 2) == 3\n";
const PY_SUB: &str = "def test_sub():\n    assert add(1, -1) == 0\n";
const PY_PARAM: &str = "@pytest.mark.parametrize(\n    \"a,b\",\n    [(1, 2), (3, 4)],\n)\ndef test_sum(a, b):\n    assert add(a, b) == a + b\n";
const PY_TESTS: &str = "tests/test_calc.py";
const PY_ID: &str = "TI-edit-tests/test_calc.py";

fn python_repo(tests: &[&str]) -> Repo {
    let text = format!("import pytest\n\n{}", tests.join("\n\n"));
    Repo::new(&[(PY_TESTS, &text)])
}

fn move_python(repo: &Repo, kept: &[&str], dest: &str, moved: &str) -> Vec<IntegrityEvent> {
    repo.write(PY_TESTS, &format!("import pytest\n\n{}", kept.join("\n\n")));
    repo.write(dest, &format!("import pytest\n\n{moved}"));
    repo.events(&[])
}

#[test]
fn python_move_cancels_into_a_pytest_file_and_raises_into_a_helper() {
    let repo = python_repo(&[PY_ADD, PY_SUB]);
    assert_eq!(
        move_python(&repo, &[PY_ADD], "tests/test_more.py", PY_SUB),
        Vec::new()
    );

    let repo = python_repo(&[PY_ADD, PY_SUB]);
    let events = move_python(&repo, &[PY_ADD], "tests/helpers.py", PY_SUB);
    assert_eq!(edit_ids(&events), [PY_ID]);
}

#[test]
fn python_move_gaining_skip_raises_even_over_a_black_formatted_decorator() {
    let repo = python_repo(&[PY_ADD, PY_SUB]);
    let skipped = format!("@pytest.mark.skip\n{PY_SUB}");
    let events = move_python(&repo, &[PY_ADD], "tests/test_more.py", &skipped);
    assert_eq!(edit_ids(&events), [PY_ID]);

    let repo = python_repo(&[PY_ADD, PY_PARAM]);
    let events = move_python(&repo, &[PY_ADD], "tests/test_more.py", PY_PARAM);
    assert_eq!(events, Vec::new(), "a verbatim parametrized move");

    let repo = python_repo(&[PY_ADD, PY_PARAM]);
    let skipped = format!("@pytest.mark.skip\n{PY_PARAM}");
    let events = move_python(&repo, &[PY_ADD], "tests/test_more.py", &skipped);
    assert_eq!(edit_ids(&events), [PY_ID]);
}

#[test]
fn go_move_cancels_verbatim_and_raises_when_the_body_gains_a_skip() {
    let header = "package calc\n\nimport \"testing\"\n\n";
    let adds =
        "func TestAdds(t *testing.T) {\n\tif Add(1, 2) != 3 {\n\t\tt.Errorf(\"bad\")\n\t}\n}\n";
    let subs = "func TestSubtracts(t *testing.T) {\n\tif Add(1, -1) != 0 {\n\t\tt.Errorf(\"bad\")\n\t}\n}\n";
    let skipped = subs.replace("{\n\tif", "{\n\tt.Skip()\n\tif");
    let run = |moved: &str| {
        let repo = Repo::new(&[("calc_test.go", &format!("{header}{adds}\n{subs}"))]);
        repo.write("calc_test.go", &format!("{header}{adds}"));
        repo.write("calc_more_test.go", &format!("{header}{moved}"));
        repo.events(&[])
    };

    assert_eq!(run(subs), Vec::new());
    assert_eq!(edit_ids(&run(&skipped)), ["TI-edit-calc_test.go"]);
}

/// The edit event of TESTS after DOUBLES is dropped for good and SUBTRACTS
/// moves into MORE, with ADDS staying.
fn partner_event(repo: &Repo) -> IntegrityEvent {
    repo.write(TESTS, &rust_file(&[ADDS]));
    repo.write(MORE, &rust_file(&[SUBTRACTS]));
    repo.write("src/lib.rs", LIB_BOTH);
    let events = repo.events(&[]);
    assert_eq!(edit_ids(&events), [EDIT_ID], "{:?}", ids(&events));
    events
        .into_iter()
        .find(|event| event.id == EDIT_ID)
        .unwrap()
}

#[test]
fn event_with_a_cancelling_partner_pins_the_file_and_its_partner() {
    let repo = crate_repo(&[ADDS, SUBTRACTS, DOUBLES], &[]);
    let event = partner_event(&repo);

    let own = sha256_hex(&rust_file(&[ADDS]));
    let partner = sha256_hex(&rust_file(&[SUBTRACTS]));
    let text = format!("{own}\n{MORE}\t{partner}\n");
    assert_ne!(event.current_sha256, Some(own));
    assert_eq!(event.current_sha256, Some(sha256_hex(&text)));
    assert_eq!(event.detail, ["assert_eq!(compute(2, 2), 4);"]);

    repo.write(
        MORE,
        &rust_file(&[&SUBTRACTS.replace("let a = 1;", "let a = 9;")]),
    );
    let again = repo.events(&[]);
    let edited = again.iter().find(|event| event.id == EDIT_ID).unwrap();
    assert_ne!(edited.current_sha256, Some(sha256_hex(&text)));
}

#[test]
fn deleted_source_file_pins_the_literal_deleted_with_its_partner() {
    let repo = crate_repo(&[ADDS, SUBTRACTS, DOUBLES], &[]);
    std::fs::remove_file(repo.root.join(TESTS)).unwrap();
    repo.write(MORE, &rust_file(&[ADDS, SUBTRACTS]));
    repo.write("src/lib.rs", "#[cfg(test)]\nmod calc_more_tests;\n");

    let events = repo.events(&[]);

    let event = events.iter().find(|event| event.id == EDIT_ID).unwrap();
    let partner = sha256_hex(&rust_file(&[ADDS, SUBTRACTS]));
    let text = format!("deleted\n{MORE}\t{partner}\n");
    assert_eq!(event.current_sha256, Some(sha256_hex(&text)));
    assert_eq!(event.detail, ["assert_eq!(compute(2, 2), 4);"]);
}

#[test]
fn event_without_a_partner_keeps_the_plain_file_hash() {
    let repo = crate_repo(&[ADDS, DOUBLES], &[]);
    repo.write(TESTS, &rust_file(&[ADDS]));

    let events = repo.events(&[]);

    let event = events.iter().find(|event| event.id == EDIT_ID).unwrap();
    assert_eq!(event.current_sha256, Some(sha256_hex(&rust_file(&[ADDS]))));
}
