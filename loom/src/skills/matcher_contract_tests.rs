//! Contract: skill triggers match whole prompt words, trimming only edge
//! punctuation, so `go mod` never fires inside `django model` while `c++` and
//! `node.js` still match.

use std::collections::HashMap;

use super::matcher::match_skills;

fn trigger_map() -> HashMap<String, Vec<String>> {
    [
        ("auth", "auth"),
        ("go mod", "go"),
        ("c++", "cpp"),
        ("node.js", "node"),
    ]
    .into_iter()
    .map(|(trigger, skill)| (trigger.to_string(), vec![skill.to_string()]))
    .collect()
}

fn matched(text: &str) -> Vec<String> {
    let descriptions: HashMap<String, String> = ["auth", "go", "cpp", "node"]
        .into_iter()
        .map(|skill| (skill.to_string(), format!("{skill} skill")))
        .collect();
    let mut names: Vec<String> = match_skills(text, &trigger_map(), &descriptions, 5, 1.0)
        .into_iter()
        .map(|skill| skill.name)
        .collect();
    names.sort();
    names
}

#[test]
fn triggers_match_whole_words_and_ignore_edge_punctuation() {
    assert_eq!(matched("django model"), Vec::<String>::new());
    assert_eq!(matched("fix auth, now"), vec!["auth".to_string()]);
    assert_eq!(
        matched("port the c++ and node.js, code"),
        vec!["cpp".to_string(), "node".to_string()]
    );
}
