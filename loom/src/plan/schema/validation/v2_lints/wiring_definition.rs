//! A `wiring` pattern shaped like a definition (`pub fn name(`, `mod x;`,
//! `def name`) can only match where a name is declared, never where it is used,
//! so it passes while the name has no caller. Completion's definition-site
//! rule (`verify/goal_backward/definition_sites.rs`) rejects the same shape
//! at the end; this lint moves the refusal to plan time.

use super::{LintContext, LintFinding};

/// Words that open a declaration, each needing a gap after it.
const KEYWORDS: [&str; 11] = [
    "fn",
    "mod",
    "struct",
    "enum",
    "trait",
    "impl",
    "type",
    "def",
    "class",
    "function",
    "interface",
];

/// Words that may precede the keyword, each needing a gap after it. `default`
/// is the second word of `export default`.
const MODIFIERS: [&str; 6] = [
    "pub(crate)",
    "pub(super)",
    "pub",
    "export",
    "default",
    "async",
];

/// The escaped forms of the parenthesised visibility, accepted in a regex.
const ESCAPED_MODIFIERS: [&str; 2] = ["pub\\(crate\\)", "pub\\(super\\)"];

/// What a regex pattern may put where whitespace goes, longest first.
const REGEX_GAPS: [&str; 4] = [" +", "\\s*", "\\s+", "\\s"];

/// Longest pattern a message quotes, in characters.
const QUOTE_CHARS: usize = 80;

pub(super) fn check_definition_shaped(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    for stage in &ctx.metadata.loom.stages {
        for (idx, wiring) in stage.wiring.iter().enumerate() {
            let Some(keyword) = definition_keyword(&wiring.pattern, wiring.literal) else {
                continue;
            };
            let message = format!(
                "Wiring #{} pattern `{}` starts with the definition keyword `{keyword}`, so it \
                 can only match where a name is declared, never where it is used; point it at \
                 the call, mount or dispatch site, prove the reach with `reachable`, or declare \
                 `consumer_deferred` when the consumer arrives in a later plan",
                idx + 1,
                quoted(&wiring.pattern)
            );
            out.push(LintFinding::in_stage(stage, message, true));
        }
    }
}

/// `pattern` cut to [`QUOTE_CHARS`] characters.
fn quoted(pattern: &str) -> String {
    match pattern.char_indices().nth(QUOTE_CHARS) {
        Some((end, _)) => format!("{}...", &pattern[..end]),
        None => pattern.to_string(),
    }
}

/// The definition keyword `pattern` starts with: after leading whitespace
/// (and, in a regex, an anchor or `\s` tokens) and any visibility or
/// `async` modifiers, one of [`KEYWORDS`] followed by a gap.
fn definition_keyword(pattern: &str, literal: bool) -> Option<&'static str> {
    let regex = !literal;
    let mut rest = pattern.trim_start();
    if regex {
        rest = skip_anchor(rest);
    }
    while let Some(after) = skip_modifier(rest, regex) {
        rest = after;
    }
    KEYWORDS.into_iter().find(|keyword| {
        rest.strip_prefix(keyword)
            .and_then(|r| skip_gap(r, regex))
            .is_some()
    })
}

/// `text` without leading whitespace, `^` and the `\s` tokens.
fn skip_anchor(text: &str) -> &str {
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let next = trimmed
            .strip_prefix('^')
            .or_else(|| REGEX_GAPS[1..].iter().find_map(|t| trimmed.strip_prefix(t)))
            .unwrap_or(trimmed);
        if next.len() == rest.len() {
            return rest;
        }
        rest = next;
    }
}

/// `text` after one modifier and the gap that follows it.
fn skip_modifier(text: &str, regex: bool) -> Option<&str> {
    let escaped = ESCAPED_MODIFIERS.iter().filter(|_| regex);
    MODIFIERS.iter().chain(escaped).find_map(|word| {
        text.strip_prefix(word)
            .and_then(|rest| skip_gap(rest, regex))
    })
}

/// `text` after at least one whitespace character, or in a regex one of
/// [`REGEX_GAPS`] as well; `None` when `text` starts with neither.
fn skip_gap(text: &str, regex: bool) -> Option<&str> {
    let mut rest = text;
    let mut seen = false;
    loop {
        let token = REGEX_GAPS
            .iter()
            .filter(|_| regex)
            .find_map(|token| rest.strip_prefix(token));
        match token.or_else(|| rest.strip_prefix(char::is_whitespace)) {
            Some(after) => {
                rest = after;
                seen = true;
            }
            None => return seen.then_some(rest),
        }
    }
}

#[cfg(test)]
#[path = "wiring_definition_tests.rs"]
mod tests;
