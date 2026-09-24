use super::*;
use crate::plan::schema::WiringCheck;
use crate::verify::goal_backward::wiring_v2::{verify_check, Pass};
use crate::verify::goal_backward::{verify_wiring, VerificationGap};
use std::fs;
use std::path::PathBuf;

const DEFINITION: &str = "pub fn country_lens_table() {}\n";

/// The definition, then a call to it from `app` on line 4.
const DEFINITION_THEN_CALL: &str =
    "pub fn country_lens_table() {}\n\nfn app() {\n    country_lens_table();\n}\n";

fn check(source: &str, pattern: &str) -> WiringCheck {
    WiringCheck {
        source: source.to_string(),
        pattern: pattern.to_string(),
        description: "table is mounted".to_string(),
        literal: false,
    }
}

/// A temp tree holding each `(relative path, content)` pair.
fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    root
}

fn verify(root: &tempfile::TempDir, source: &str, plan_version: u32) -> Vec<VerificationGap> {
    let wiring = [check(source, "country_lens_table")];
    verify_wiring(&wiring, root.path(), plan_version).unwrap()
}

/// Verify `pattern` over `source` under the v2 rules.
fn verify_v2(root: &tempfile::TempDir, source: &str, pattern: &str) -> Vec<VerificationGap> {
    verify_wiring(&[check(source, pattern)], root.path(), 2).unwrap()
}

#[test]
fn wiring_match_only_at_definition_is_a_gap() {
    let root = tree(&[("src/table.rs", DEFINITION)]);

    let gaps = verify(&root, "src/**/*.rs", 2);

    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert!(
        gaps[0].description.contains(
            "pattern matches only the definition of country_lens_table in src/table.rs; \
             point it at a consumer"
        ),
        "{gaps:?}"
    );
}

#[test]
fn wiring_match_at_consumer_passes() {
    let root = tree(&[
        ("src/table.rs", DEFINITION),
        ("src/app.rs", "fn main() {\n    country_lens_table();\n}\n"),
    ]);

    let gaps = verify(&root, "src/**/*.rs", 2);

    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn v1_wiring_ignores_definition_sites() {
    let root = tree(&[("src/table.rs", DEFINITION)]);

    let gaps = verify(&root, "src/table.rs", 1);

    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn multi_line_match_ending_on_definition_is_excluded() {
    // The match starts on line 1, but its only occurrence of a defined name is
    // the definition on line 3.
    let content = "// header\n// header\npub fn country_lens_table() {}\n";
    let root = tree(&[("src/table.rs", content)]);

    let gaps = verify_v2(&root, "src/table.rs", r"[\s\S]*country_lens_table");

    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert!(
        gaps[0]
            .description
            .contains("pattern matches only the definition of country_lens_table in src/table.rs"),
        "{gaps:?}"
    );
}

#[test]
fn same_file_call_beside_definition_counts() {
    let root = tree(&[("src/table.rs", DEFINITION_THEN_CALL)]);

    let gaps = verify_v2(&root, "src/table.rs", "country_lens_table");

    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn match_spanning_definition_and_call_counts() {
    let root = tree(&[("src/table.rs", DEFINITION_THEN_CALL)]);

    let gaps = verify_v2(
        &root,
        "src/table.rs",
        r"country_lens_table[\s\S]*country_lens_table",
    );

    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn definition_line_with_call_of_defined_name_counts() {
    // The match starts on `app`'s definition line and names `app`, but its
    // `country_lens_table` is a call: that name is defined on line 1.
    let content = "pub fn country_lens_table() {}\nfn app() { country_lens_table(); }\n";
    let root = tree(&[("src/table.rs", content)]);

    let gaps = verify_v2(&root, "src/table.rs", r"fn app\(\) \{ country_lens_table");

    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn definition_line_of_another_name_still_counts() {
    // `table` is defined on the matching line, but only as a fragment of the
    // matched identifier, so the call is a consumer.
    let root = tree(&[("src/table.rs", "fn table() { country_lens_table(); }\n")]);

    let gaps = verify(&root, "src/table.rs", 2);

    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn file_without_extractor_keeps_its_matches() {
    let root = tree(&[
        ("src/table.rs", DEFINITION),
        ("src/notes.md", "Mount country_lens_table here.\n"),
    ]);

    let gaps = verify(&root, "src/**/*", 2);

    assert!(gaps.is_empty(), "{gaps:?}");
}

#[test]
fn definition_only_gap_omits_files_without_a_match() {
    let root = tree(&[
        ("src/table.rs", DEFINITION),
        ("src/notes.md", "Nothing mounted yet.\n"),
    ]);

    let gaps = verify(&root, "src/**/*", 2);

    assert_eq!(gaps.len(), 1, "{gaps:?}");
    assert!(
        gaps[0]
            .description
            .ends_with("point it at a consumer (table is mounted)"),
        "{gaps:?}"
    );
}

#[test]
fn pass_resting_only_on_unchecked_files_is_flagged() {
    let notes = ("src/notes.md", "Mount country_lens_table here.\n");
    let table = ("src/table.rs", DEFINITION);
    // Sorts after `notes.md`, so the scan must go on past the unchecked pass.
    let view = ("src/view.rs", "fn view() { country_lens_table(); }\n");
    let unchecked = tree(&[notes, table]);
    let checked = tree(&[notes, table, view]);
    let wiring = check("src/**/*", "country_lens_table");

    assert_eq!(
        verify_check(&wiring, unchecked.path()).unwrap(),
        Pass::UncheckedOnly(vec![PathBuf::from("src/notes.md")])
    );
    assert_eq!(
        verify_check(&wiring, checked.path()).unwrap(),
        Pass::Checked
    );
}

#[test]
fn definition_lines_name_each_symbol_on_its_first_line() {
    let bytes = b"struct Widget;\n\nfn build() {}\n";

    let lines = definition_lines(Path::new("src/widget.rs"), bytes);

    assert_eq!(
        lines,
        Some(vec![(1, "Widget".to_string()), (3, "build".to_string())])
    );
    assert_eq!(definition_lines(Path::new("notes.md"), bytes), None);
}

#[test]
fn degraded_extraction_is_unchecked() {
    let lines = definition_lines(Path::new("src/broken.rs"), b"fn broken( {\n");

    assert_eq!(lines, None);
}
