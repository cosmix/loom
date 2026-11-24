//! Contract: a review round survives a change only when every changed path is a frozen file at
//! exactly its frozen digest.

use std::collections::BTreeMap;

use super::frozen_cover::only_intact_frozen;

fn digests(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(path, digest)| (path.to_string(), digest.to_string()))
        .collect()
}

fn paths(list: &[&str]) -> Vec<String> {
    list.iter().map(|path| path.to_string()).collect()
}

#[test]
fn only_an_intact_frozen_file_keeps_the_round() {
    let frozen = digests(&[("tests/c.rs", "aa")]);
    let cases = [
        (
            "a frozen file at its frozen digest",
            paths(&["tests/c.rs"]),
            digests(&[("tests/c.rs", "aa")]),
            true,
        ),
        (
            "a frozen file at another digest",
            paths(&["tests/c.rs"]),
            digests(&[("tests/c.rs", "bb")]),
            false,
        ),
        (
            "an intact frozen file beside a changed source file",
            paths(&["tests/c.rs", "src/x.rs"]),
            digests(&[("tests/c.rs", "aa"), ("src/x.rs", "cc")]),
            false,
        ),
        ("an empty change set", Vec::new(), BTreeMap::new(), false),
        (
            "a deleted frozen file",
            paths(&["tests/c.rs"]),
            digests(&[("tests/c.rs", "deleted")]),
            false,
        ),
    ];
    for (what, changed, current, expected) in cases {
        assert_eq!(
            only_intact_frozen(&changed, &current, &frozen),
            expected,
            "{what}"
        );
    }
}
