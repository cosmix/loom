//! Cross-file move cancellation over hand-built deltas and an in-memory head
//! tree.

use super::super::count::matchers;
use super::super::edits::{statements_of, unmatched};
use super::super::statements::assertion_statements;
use super::*;
use crate::testrun::languages;
use std::collections::BTreeMap;

const CARGO: &str = "[package]\nname = \"calc\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const LIB: &str = "#[cfg(test)]\nmod calc_tests;\n#[cfg(test)]\nmod calc_more_tests;\n";
const LIB_ONE: &str = "#[cfg(test)]\nmod calc_tests;\n";
const SOURCE: &str = "src/calc_tests.rs";
const DEST: &str = "src/calc_more_tests.rs";
const ADDS: &str = "#[test]\nfn adds() {\n    assert_eq!(compute(1, 2), 3);\n}\n";
const SUBTRACTS: &str =
    "#[test]\nfn subtracts() {\n    let a = 3;\n    assert_eq!(compute(a, 1), 2);\n}\n";

fn file(fns: &[&str]) -> String {
    format!("use crate::compute;\n\n{}", fns.join("\n"))
}

fn delta(path: &str, base: &str, head: &str) -> FileDelta {
    let profile = languages::for_path(path).expect("profiled path");
    let assertion = &matchers(profile).assertion;
    let before = statements_of(base, assertion, profile.name);
    let after = statements_of(head, assertion, profile.name);
    FileDelta {
        path: path.to_string(),
        profile,
        existed_at_base: !base.is_empty(),
        base: base.to_string(),
        head: head.to_string(),
        lost: unmatched(&before, &after),
        added: unmatched(&after, &before),
        siblings: Vec::new(),
    }
}

fn tree(entries: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let files: BTreeMap<String, String> = entries
        .iter()
        .map(|(path, content)| (path.to_string(), content.to_string()))
        .collect();
    move |path: &str| files.get(path).cloned()
}

/// `SUBTRACTS` moves from the source to the destination, which `lib` declares.
fn moved(dest_text: &str, lib: &str) -> (Vec<FileDelta>, BTreeMap<String, BTreeSet<String>>) {
    let source_head = file(&[ADDS]);
    let mut deltas = vec![
        delta(SOURCE, &file(&[ADDS, SUBTRACTS]), &source_head),
        delta(DEST, "", dest_text),
    ];
    let head = tree(&[
        ("Cargo.toml", CARGO),
        ("src/lib.rs", lib),
        (SOURCE, &source_head),
        (DEST, dest_text),
    ]);
    let partners = cancel_moves(&mut deltas, &head);
    (deltas, partners)
}

#[test]
fn verbatim_move_into_a_declared_file_cancels_both_sides() {
    let (deltas, partners) = moved(&file(&[SUBTRACTS]), LIB);

    assert!(deltas[0].lost.is_empty() && deltas[1].added.is_empty());
    let expected = BTreeSet::from([DEST.to_string()]);
    assert_eq!(partners, BTreeMap::from([(SOURCE.to_string(), expected)]));
}

#[test]
fn move_into_a_file_no_mod_reaches_cancels_nothing() {
    let (deltas, partners) = moved(&file(&[SUBTRACTS]), LIB_ONE);

    assert_eq!(deltas[0].lost.len(), 1);
    assert_eq!(deltas[1].added.len(), 1);
    assert!(partners.is_empty());
}

#[test]
fn changed_body_line_beside_an_identical_assert_cancels_nothing() {
    let edited = SUBTRACTS.replace("let a = 3;", "let a = 4;");
    let (deltas, partners) = moved(&file(&[&edited]), LIB);

    assert_eq!(deltas[0].lost.len(), 1);
    assert!(partners.is_empty());
}

#[test]
fn gained_ignore_attribute_cancels_nothing() {
    let ignored = format!("#[ignore]\n{SUBTRACTS}");
    let (deltas, partners) = moved(&file(&[&ignored]), LIB);

    assert_eq!(deltas[0].lost.len(), 1);
    assert!(partners.is_empty());
}

#[test]
fn enclosing_mod_may_differ_between_the_files() {
    let nested = format!("mod inner {{\n    use crate::compute;\n{SUBTRACTS}}}\n");
    let (deltas, partners) = moved(&nested, LIB);

    assert!(deltas[0].lost.is_empty());
    assert_eq!(partners.len(), 1);
}

#[test]
fn no_extractors_cancel_nothing() {
    let source_head = file(&[ADDS]);
    let dest = file(&[SUBTRACTS]);
    let mut deltas = vec![
        delta(SOURCE, &file(&[ADDS, SUBTRACTS]), &source_head),
        delta(DEST, "", &dest),
    ];
    let head = tree(&[("Cargo.toml", CARGO), ("src/lib.rs", LIB)]);

    assert!(cancel_with(&[], &mut deltas, &head).is_empty());
    assert_eq!(deltas[0].lost.len(), 1);
}

#[test]
fn kotlin_without_a_grammar_cancels_nothing() {
    let test = "class T {\n    @Test\n    fun a() {\n        assertEquals(1, x)\n    }\n}\n";
    let mut deltas = vec![
        delta("src/test/FooTest.kt", test, "class T {}\n"),
        delta("src/test/BarTest.kt", "", test),
    ];

    let partners = cancel_moves(&mut deltas, &tree(&[]));

    assert!(partners.is_empty());
    assert_eq!(deltas[0].lost.len(), 1);
}

#[test]
fn module_level_python_assert_has_no_function_and_cancels_nothing() {
    let module_level = "assert add(1, 2) == 3\n";
    let mut deltas = vec![
        delta("tests/test_a.py", module_level, ""),
        delta("tests/test_b.py", "", module_level),
    ];

    assert!(cancel_moves(&mut deltas, &tree(&[])).is_empty());
    assert_eq!(deltas[0].lost.len(), 1);
}

#[test]
fn python_moves_follow_the_runner_default_file_names() {
    let test = "def test_sub():\n    assert add(1, -1) == 0\n";
    let keep = "def test_add():\n    assert add(1, 2) == 3\n";
    let run = |dest: &str| {
        let mut deltas = vec![
            delta("tests/test_calc.py", &format!("{keep}\n{test}"), keep),
            delta(dest, "", test),
        ];
        cancel_moves(&mut deltas, &tree(&[])).len()
    };

    assert_eq!(run("tests/test_more.py"), 1);
    assert_eq!(run("tests/more_test.py"), 1);
    assert_eq!(run("tests/helpers.py"), 0);
}

#[test]
fn path_attribute_hop_reaches_a_file_a_reached_file_declares() {
    let source_head = file(&[ADDS]);
    let dest = file(&[SUBTRACTS]);
    let mut deltas = vec![
        delta(SOURCE, &file(&[ADDS, SUBTRACTS]), &source_head),
        delta("src/split_tests.rs", "", &dest),
    ];
    deltas[1].siblings = vec!["src/calc.rs".to_string(), "src/lib.rs".to_string()];
    let declaring = "#[cfg(test)]\n#[path = \"split_tests.rs\"]\nmod split;\n";
    let head = tree(&[
        ("Cargo.toml", CARGO),
        ("src/lib.rs", "mod calc;\n#[cfg(test)]\nmod calc_tests;\n"),
        ("src/calc.rs", declaring),
        (SOURCE, &source_head),
        ("src/split_tests.rs", &dest),
    ]);

    assert_eq!(cancel_moves(&mut deltas, &head).len(), 1);

    let gated = declaring.replace("#[cfg(test)]", "#[cfg(feature = \"x\")]");
    let mut again = vec![
        delta(SOURCE, &file(&[ADDS, SUBTRACTS]), &source_head),
        delta("src/split_tests.rs", "", &dest),
    ];
    again[1].siblings = vec!["src/calc.rs".to_string()];
    let head = tree(&[
        ("Cargo.toml", CARGO),
        ("src/lib.rs", "mod calc;\n"),
        ("src/calc.rs", &gated),
    ]);
    assert!(cancel_moves(&mut again, &head).is_empty());
}

fn prefix_of(source: &str, function: &str) -> String {
    let at = source.find(function).expect("function in source");
    let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
    decoration_prefix(&source[..line_start])
}

#[test]
fn prefix_takes_a_multi_line_attribute_whole() {
    let source =
        "// note\n#[test]\n#[cfg_attr(\n    feature = \"slow\",\n    ignore\n)]\nfn t() {}\n";

    let prefix = prefix_of(source, "fn t");

    assert_eq!(
        prefix,
        "#[test]\n#[cfg_attr(\n    feature = \"slow\",\n    ignore\n)]"
    );
}

#[test]
fn prefix_takes_a_black_formatted_decorator_whole() {
    let source = "x = 1\n\n@pytest.mark.parametrize(\n    \"a,b\",\n    [(1, 2), (3, 4)],\n)\ndef test_sum(a, b):\n    pass\n";

    let prefix = prefix_of(source, "def test_sum");

    assert_eq!(
        prefix,
        "@pytest.mark.parametrize(\n    \"a,b\",\n    [(1, 2), (3, 4)],\n)"
    );
}

#[test]
fn prefix_walks_over_blank_and_comment_lines_and_stops_at_code() {
    // Blank and comment lines are walked over and left out of the identity.
    let source = "#[test]\n\n#[ignore]\nfn t() {}\n";
    assert_eq!(prefix_of(source, "fn t"), "#[test]\n#[ignore]");

    let python = "@pytest.mark.skip\n# why\ndef test_x():\n    pass\n";
    assert_eq!(prefix_of(python, "def test_x"), "@pytest.mark.skip");

    let block = "#[ignore]\n/* slow */\n// note\n#![allow(x)]\nfn t() {}\n";
    assert_eq!(prefix_of(block, "fn t"), "#[ignore]\n#![allow(x)]");

    // Any other code line ends the walk, a plain closing bracket included.
    let code = "#[ignore]\nlet x = 1;\n\n#[test]\nfn t() {}\n";
    assert_eq!(prefix_of(code, "fn t"), "#[test]");

    let call = "foo(\n    1,\n)\nfn t() {}\n";
    assert_eq!(prefix_of(call, "fn t"), "");
}

#[test]
fn gained_ignore_separated_by_a_blank_or_comment_line_cancels_nothing() {
    for separator in ["\n", "// slow\n"] {
        let ignored = format!("#[ignore]\n{separator}{SUBTRACTS}");
        let (deltas, partners) = moved(&file(&[&ignored]), LIB);

        assert_eq!(deltas[0].lost.len(), 1, "{separator:?}");
        assert!(partners.is_empty(), "{separator:?}");
    }
}

#[test]
fn a_move_into_a_file_of_another_language_does_not_cancel() {
    let source_head = file(&[ADDS]);
    let dest = file(&[SUBTRACTS]);
    let mut deltas = vec![
        delta(SOURCE, &file(&[ADDS, SUBTRACTS]), &source_head),
        delta(DEST, "", &dest),
    ];
    // Only the language comparison can block this: statement key, function
    // identity and (for a Rust profile) collection all match.
    deltas[1].profile = languages::for_path("src/calc_more_test.go").expect("go profile");
    let head = tree(&[
        ("Cargo.toml", CARGO),
        ("src/lib.rs", LIB),
        (SOURCE, &source_head),
        (DEST, &dest),
    ]);

    assert!(cancel_moves(&mut deltas, &head).is_empty());
    assert_eq!(deltas[0].lost.len(), 1);
    assert_eq!(deltas[1].added.len(), 1);
}

#[test]
fn statements_of_a_hand_built_delta_index_its_own_sides() {
    let delta = delta(SOURCE, &file(&[ADDS, SUBTRACTS]), &file(&[ADDS]));
    let assertion = &matchers(delta.profile).assertion;
    let base = assertion_statements(&delta.base, assertion, "rust");

    assert_eq!(base.len(), 2);
    assert_eq!(delta.lost.len(), 1);
    assert!(delta.base[delta.lost[0].start..].starts_with("assert_eq!(compute(a, 1)"));
}

const TS_SOURCE: &str = "web/src/calc.test.ts";
const TS_DEST: &str = "web/src/more.test.ts";
const TS_KEEP: &str = "const keeps = () => {\n  expect(compute(1, 2)).toBe(3);\n};\n";
const TS_MOVED: &str = "const subtracts = () => {\n  expect(compute(1, -1)).toBe(0);\n};\n";

/// `TS_MOVED` leaves `TS_SOURCE` for `TS_DEST` as `moved`; the lost statement
/// is still lost afterwards when nothing cancelled it.
fn move_typescript(moved: &str) -> (Vec<FileDelta>, usize) {
    let mut deltas = vec![
        delta(TS_SOURCE, &format!("{TS_KEEP}{TS_MOVED}"), TS_KEEP),
        delta(TS_DEST, "", moved),
    ];
    let partners = cancel_moves(&mut deltas, &tree(&[])).len();
    (deltas, partners)
}

#[test]
fn named_typescript_function_moved_verbatim_cancels() {
    let (deltas, partners) = move_typescript(TS_MOVED);

    assert_eq!(partners, 1);
    assert!(deltas[0].lost.is_empty() && deltas[1].added.is_empty());
}

#[test]
fn typescript_move_gaining_a_skip_wrapper_on_its_first_line_cancels_nothing() {
    let skipped = format!("describe.skip(\"s\", () => {{ {TS_MOVED}}});\n");

    let (deltas, partners) = move_typescript(&skipped);

    assert_eq!(partners, 0);
    assert_eq!(deltas[0].lost.len(), 1);
    assert_eq!(deltas[1].added.len(), 1);
}

#[test]
fn anonymous_callback_test_never_cancels() {
    let callback = "it(\"subtracts\", () => {\n  expect(compute(1, -1)).toBe(0);\n});\n";
    let mut deltas = vec![
        delta(TS_SOURCE, &format!("{TS_KEEP}{callback}"), TS_KEEP),
        delta(TS_DEST, "", callback),
    ];

    assert!(cancel_moves(&mut deltas, &tree(&[])).is_empty());
    assert_eq!(deltas[0].lost.len(), 1);
}

/// Cancellations of `SUBTRACTS` moved to `src/split_tests.rs`, which
/// `src/calc.rs` (reached by a `mod` chain) declares with `declaring`.
fn hop_through(declaring: &str) -> usize {
    let source_head = file(&[ADDS]);
    let dest = file(&[SUBTRACTS]);
    let mut deltas = vec![
        delta(SOURCE, &file(&[ADDS, SUBTRACTS]), &source_head),
        delta("src/split_tests.rs", "", &dest),
    ];
    deltas[1].siblings = vec!["src/calc.rs".to_string()];
    let head = tree(&[
        ("Cargo.toml", CARGO),
        ("src/lib.rs", "mod calc;\n#[cfg(test)]\nmod calc_tests;\n"),
        ("src/calc.rs", declaring),
        (SOURCE, &source_head),
        ("src/split_tests.rs", &dest),
    ]);
    cancel_moves(&mut deltas, &head).len()
}

#[test]
fn path_declaration_rustc_never_compiles_does_not_collect_the_moved_file() {
    let live = "#[cfg(test)]\n#[path = \"split_tests.rs\"]\nmod split;\n";
    assert_eq!(hop_through(live), 1);

    let commented = format!("/*\n{live}*/\n");
    assert_eq!(hop_through(&commented), 0, "block comment");
    let off = "#[cfg(any())]\n#[path = \"split_tests.rs\"]\nmod split;\n";
    assert_eq!(hop_through(off), 0, "cfg(any())");
    let nested = format!("#[cfg(any())]\nmod off {{\n{live}}}\n");
    assert_eq!(hop_through(&nested), 0, "inside a gated inline module");
    let raw = format!("const TEXT: &str = r#\"\n{live}\"#;\n");
    assert_eq!(hop_through(&raw), 0, "raw string");
    let in_macro = format!("macro_rules! declare {{\n    () => {{\n{live}    }};\n}}\n");
    assert_eq!(hop_through(&in_macro), 0, "macro body");
}
