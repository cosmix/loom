//! The assertion statements one file's text loses between two versions, read
//! with the detector the integrity gate uses (`edits`).

use super::count::matchers;
use super::edits::{statements_of, unmatched};
use crate::testrun::languages;

/// The assertion statements of `before` that `after` does not hold, in source
/// order. A path whose extension has no language profile gives an empty list.
pub fn lost_statements(path: &str, before: &str, after: &str) -> Vec<String> {
    let Some(profile) = languages::for_path(path) else {
        return Vec::new();
    };
    let assertion = &matchers(profile).assertion;
    let kept = statements_of(after, assertion, profile.name);
    let frozen = statements_of(before, assertion, profile.name);
    unmatched(&frozen, &kept)
        .into_iter()
        .map(|statement| statement.text)
        .collect()
}
