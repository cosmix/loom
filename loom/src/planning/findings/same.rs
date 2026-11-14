//! Whether two ledger entries report the same thing.
//!
//! Reviewers raise one issue in several wordings: two review rounds of a stage, a stage and its
//! integration verification, the run review of the next plan. Two entries are the same finding
//! when they point at the same place and most of the smaller claim's words are in the larger one.
//! The place is the cited `path:line` when both texts lead with one (the same file, lines at
//! most [`NEAR_LINES`] apart); when only one does, the same `Evidence:` quote; when neither does,
//! that quote or the same primary node: each entry's first `About:` token is among the other's
//! tokens. The first token is the
//! node the change belongs in (an imported entry's is its plan stage), so the plan's
//! `Implements:` tokens every imported entry repeats never place two entries together. A shared
//! quote alone is not enough: one sentence can show two problems. The measure is deterministic
//! and cheap; it misses a rewording that shares few words, which a review session still catches
//! by reading the ledger. Revisit the thresholds when a real ledger shows two distinct findings
//! merged or one finding kept twice.

use std::collections::BTreeSet;

use super::Finding;

/// Words too common to tell two claims apart.
const STOP_WORDS: [&str; 34] = [
    "the", "and", "for", "with", "that", "this", "from", "but", "not", "are", "was", "its", "has",
    "have", "into", "only", "when", "which", "while", "would", "could", "should", "also", "then",
    "than", "does", "each", "one", "use", "uses", "still", "gives", "there", "they",
];
/// Two claims must share at least this many words.
const MIN_SHARED: usize = 5;
/// The share of the smaller claim's words the larger one must hold, as a fraction.
const SHARE: (usize, usize) = (3, 5);
/// How far apart two cited lines of one file may be: reviewers cite the line of a call, of its
/// function or of the condition, which a run review's reworded item moves by up to about twenty.
const NEAR_LINES: u64 = 30;

/// Whether `a` and `b` report the same thing.
pub fn same_finding(a: &Finding, b: &Finding) -> bool {
    let same_quote = match (&a.evidence, &b.evidence) {
        (Some(x), Some(y)) => {
            let x = normalized_quote(x);
            !x.is_empty() && x == normalized_quote(y)
        }
        _ => false,
    };
    let same_place = match (cited(&a.text), cited(&b.text)) {
        (Some((x, i)), Some((y, j))) => x == y && i.abs_diff(j) <= NEAR_LINES,
        (None, None) => same_quote || same_primary_node(a, b),
        _ => same_quote,
    };
    same_place && similar(&claim_words(a), &claim_words(b))
}

/// Each entry's first `About:` token is one of the other's tokens.
fn same_primary_node(a: &Finding, b: &Finding) -> bool {
    let holds = |x: &Finding, y: &Finding| x.about.first().is_some_and(|t| y.about.contains(t));
    holds(a, b) && holds(b, a)
}

/// A quote as the evidence check compares it: Markdown emphasis and code marks and surrounding
/// quotation marks removed, whitespace runs collapsed to one space, lowercase.
pub fn normalized_quote(text: &str) -> String {
    let marks = |c: char| matches!(c, '*' | '_' | '`');
    let quotes: &[char] = &['"', '\'', '\u{201c}', '\u{201d}', '\u{2018}', '\u{2019}'];
    let plain: String = text.chars().filter(|c| !marks(*c)).collect();
    let words: Vec<&str> = plain
        .trim()
        .trim_matches(quotes)
        .split_whitespace()
        .collect();
    words.join(" ").to_lowercase()
}

/// The file and line a review item cites first, from a leading `path:line`.
fn cited(text: &str) -> Option<(&str, u64)> {
    let first = text.split_whitespace().next()?;
    let (path, line) = first.rsplit_once(':')?;
    let line = line.trim_end_matches([',', ';', '.']).parse().ok()?;
    path.contains('.').then_some((path, line))
}

/// The claim's distinctive words: the title and the text, split on anything not a letter, digit
/// or `_`, lowercase, with words under three letters, numbers, [`STOP_WORDS`] and the words of
/// the cited path dropped. An imported entry's title repeats its text's leading `path:line`, and
/// two entries are compared only when that path is the same, so its words would count twice.
fn claim_words(finding: &Finding) -> BTreeSet<String> {
    let path: BTreeSet<String> = cited(&finding.text)
        .map(|(path, _)| words(path))
        .unwrap_or_default();
    let claim = format!("{} {}", finding.title, finding.text);
    let mut found = words(&claim);
    found.retain(|word| !path.contains(word));
    found
}

fn words(text: &str) -> BTreeSet<String> {
    let lower = text.to_lowercase();
    lower
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| word.chars().count() >= 3)
        .filter(|word| !word.chars().all(|c| c.is_ascii_digit()))
        .filter(|word| !STOP_WORDS.contains(word))
        .map(str::to_owned)
        .collect()
}

/// At least [`MIN_SHARED`] shared words, and at least [`SHARE`] of the smaller set shared.
fn similar(a: &BTreeSet<String>, b: &BTreeSet<String>) -> bool {
    let shared = a.intersection(b).count();
    let smaller = a.len().min(b.len());
    let (part, whole) = SHARE;
    shared >= MIN_SHARED && shared * whole >= smaller * part
}

#[cfg(test)]
#[path = "same_tests.rs"]
mod tests;
