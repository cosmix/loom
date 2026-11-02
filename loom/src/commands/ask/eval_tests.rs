use std::fs;
use std::path::PathBuf;

use super::{parse_cases, EvalCase};

const NOT_FOUND: &str = "not found";
const EXPECTED_CASES: usize = 24;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ask_eval")
}

fn load_cases() -> Vec<EvalCase> {
    let text = fs::read_to_string(fixture_dir().join("cases.json")).expect("read cases.json");
    parse_cases(&text).expect("cases.json parses")
}

/// The text of every file a case lists, lowercased for ASCII-insensitive
/// matching like `first_miss` does.
fn case_text(case: &EvalCase) -> String {
    let mut text = String::new();
    for path in &case.paths {
        let file = fixture_dir().join(path);
        let body = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("case {} lists {path}: {error}", case.id));
        text.push_str(&body.to_ascii_lowercase());
        text.push('\n');
    }
    text
}

#[test]
fn every_case_can_be_answered_from_its_files() {
    let cases = load_cases();
    assert_eq!(cases.len(), EXPECTED_CASES);
    for case in &cases {
        let text = case_text(case);
        for needle in case.must_contain.iter().filter(|s| s.as_str() != NOT_FOUND) {
            assert!(
                text.contains(&needle.to_ascii_lowercase()),
                "case {}: {needle:?} occurs in none of {:?}",
                case.id,
                case.paths
            );
        }
    }
}

#[test]
fn the_fixtures_hold_no_personal_path_or_name() {
    let dir = fixture_dir();
    let mut files = vec![dir.join("cases.json")];
    for case in load_cases() {
        files.extend(case.paths.iter().map(|path| dir.join(path)));
    }
    for file in files {
        let text = fs::read_to_string(&file).expect("read fixture");
        let scrubbed = text.replace("/home/user", "");
        assert!(
            !scrubbed.contains("/home/"),
            "{} holds a home path",
            file.display()
        );
        // The encoded project-directory form (`-home-<name>-`); a bare name is
        // an ordinary word for some accounts, so only the path form is checked.
        if let Some(user) = std::env::var("USER").ok().filter(|user| !user.is_empty()) {
            let encoded = format!("-home-{}-", user.to_ascii_lowercase());
            assert!(
                !text
                    .to_ascii_lowercase()
                    .replace("-home-user-", "")
                    .contains(&encoded),
                "{} names the user in an encoded home path",
                file.display()
            );
        }
    }
}

#[test]
fn a_duplicate_case_id_is_rejected() {
    let case = r#"{"id":"same","question":"q","paths":["a.md"],"must_contain":["x"]}"#;
    let text = format!("[{case},{case}]");
    let error = parse_cases(&text).expect_err("duplicate ids are refused");
    assert!(error.to_string().contains("duplicate case id"), "{error:#}");
}
