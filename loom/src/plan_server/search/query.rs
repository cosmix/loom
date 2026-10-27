//! Search query parsing and snippets.

use crate::context::lexical::tokenize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Literal(String),
    Terms(Vec<String>),
}

pub fn parse_query(q: &str) -> Query {
    let q = q.trim();
    if q.len() >= 2 && q.starts_with('"') && q.ends_with('"') {
        Query::Literal(q[1..q.len() - 1].to_owned())
    } else {
        Query::Terms(tokenize(q))
    }
}

/// Count non-overlapping, case-sensitive literal matches.
pub fn literal_count(text: &str, needle: &str) -> usize {
    if needle.is_empty() {
        0
    } else {
        text.matches(needle).count()
    }
}

/// Return a character-bounded window around the first matching term.
pub fn snippet(body: &str, terms: &[String], max_chars: usize) -> String {
    if max_chars == 0 || body.is_empty() {
        return String::new();
    }
    let lower = body.to_ascii_lowercase();
    let first = terms
        .iter()
        .filter(|term| !term.is_empty())
        .filter_map(|term| lower.find(&term.to_ascii_lowercase()))
        .min()
        .unwrap_or(0);
    let match_char = body[..first].chars().count();
    let total = body.chars().count();
    let start = match_char
        .saturating_sub(max_chars / 2)
        .min(total.saturating_sub(max_chars));
    let end = start.saturating_add(max_chars).min(total);
    let mut result = String::new();
    if start > 0 {
        result.push('…');
    }
    result.extend(body.chars().skip(start).take(end - start));
    if end < total {
        result.push('…');
    }
    result
}
