//! Tokens of the planning trace's edge lines (`Covers:`, `Implements:`, `About:`).
//!
//! A value is a comma-separated list of tokens. A token names a product section
//! (`section_04`, `section_04.md`, `section_04#anchor`), the vision, the metaplan, a plan or a
//! stage or heading of a plan, either bare or in a Markdown link whose label or target holds
//! the token. Anything else is a bad token, kept as written.

#[cfg(test)]
mod tests;

use std::path::Path;

use serde::Serialize;

use crate::fs::knowledge::splice::fence_mask;
use crate::planning::slug::plan_slug;

/// A product section, whole or narrowed to one heading.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct SectionRef {
    /// The section number of `section_NN.md`.
    pub number: u32,
    /// The GitHub anchor of one heading, or `stage:<id>`; `None` names the whole section.
    pub anchor: Option<String>,
}

/// One well-formed token of an edge line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// A product section or one of its headings.
    Section(SectionRef),
    /// The feature's vision, or one of its headings.
    Vision(Option<String>),
    /// The feature's metaplan, or one of its entries.
    Metaplan(Option<String>),
    /// A plan by slug (any lifecycle prefix), or one of its headings or stages.
    Plan {
        /// The plan's lifecycle-independent slug.
        slug: String,
        /// A heading anchor or `stage:<id>`.
        anchor: Option<String>,
    },
}

/// An `Implements (stage:<id>):` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageLine {
    /// The stage id as written.
    pub stage: String,
    /// The section tokens, in order and deduplicated.
    pub refs: Vec<SectionRef>,
    /// The tokens that are not section tokens, as written.
    pub bad: Vec<String>,
    /// Zero-based line of the declaration.
    pub line: usize,
}

/// Every token of `value` in order, deduplicated, and the malformed ones as written.
pub fn tokens(value: &str) -> (Vec<Token>, Vec<String>) {
    collect(value, Some)
}

/// The section tokens of `value` in order, deduplicated. Every other token, well-formed or not,
/// is returned as written in the second list.
pub fn section_refs(value: &str) -> (Vec<SectionRef>, Vec<String>) {
    collect(value, |token| match token {
        Token::Section(section) => Some(section),
        _ => None,
    })
}

/// The `Implements (stage:<id>):` lines of a plan: column 0, outside fenced code and before the
/// first `<!-- loom METADATA` line.
pub fn stage_lines(text: &str) -> Vec<StageLine> {
    let lines: Vec<&str> = text.lines().collect();
    let mask = fence_mask(&lines);
    let mut found = Vec::new();
    for (line, text) in lines.iter().enumerate() {
        if mask[line] {
            continue;
        }
        if text.contains("<!-- loom METADATA") {
            break;
        }
        let Some((id, value)) = text
            .strip_prefix("Implements (stage:")
            .and_then(|rest| rest.split_once("):"))
        else {
            continue;
        };
        let (refs, bad) = section_refs(value);
        found.push(StageLine {
            stage: id.trim().to_owned(),
            refs,
            bad,
            line,
        });
    }
    found
}

/// `PLAN-<slug>.md` alone: one plan token with no anchor and no lifecycle prefix, whose slug
/// holds letters, digits, `-`, `_` and inner dots.
pub fn is_plan_token(token: &str) -> bool {
    let (found, bad) = tokens(token);
    let [Token::Plan { slug, anchor: None }] = found.as_slice() else {
        return false;
    };
    bad.is_empty()
        && token == format!("PLAN-{slug}.md")
        && !slug.starts_with('.')
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn collect<T: PartialEq>(value: &str, pick: impl Fn(Token) -> Option<T>) -> (Vec<T>, Vec<String>) {
    let (mut found, mut bad) = (Vec::new(), Vec::new());
    for raw in value.split(',').map(str::trim) {
        match parse_token(raw).and_then(&pick) {
            Some(item) if !found.contains(&item) => found.push(item),
            Some(_) => {}
            None => bad.push(raw.to_owned()),
        }
    }
    (found, bad)
}

fn parse_token(raw: &str) -> Option<Token> {
    match raw.strip_prefix('[') {
        Some(link) => parse_link(link),
        None => parse_plain(raw),
    }
}

/// `label](target)`: the target's file name and fragment win, then the label.
fn parse_link(link: &str) -> Option<Token> {
    let (label, target) = link.split_once("](")?;
    let target = target.strip_suffix(')')?;
    target_name(target)
        .and_then(|name| parse_plain(&name))
        .or_else(|| parse_plain(label.trim()))
}

fn target_name(target: &str) -> Option<String> {
    let (path, fragment) = match target.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (target, None),
    };
    let name = Path::new(path.split('?').next()?).file_name()?.to_str()?;
    Some(match fragment {
        Some(fragment) => format!("{name}#{fragment}"),
        None => name.to_owned(),
    })
}

fn parse_plain(raw: &str) -> Option<Token> {
    let (head, anchor) = match raw.split_once('#') {
        Some((head, anchor)) if valid_anchor(anchor) => (head, Some(anchor.to_owned())),
        Some(_) => return None,
        None => (raw, None),
    };
    if let Some(number) = section_number(head.strip_suffix(".md").unwrap_or(head)) {
        return Some(Token::Section(SectionRef { number, anchor }));
    }
    match head {
        "vision" => Some(Token::Vision(anchor)),
        "metaplan" => Some(Token::Metaplan(anchor)),
        _ => plan_slug(head).map(|slug| Token::Plan { slug, anchor }),
    }
}

fn section_number(token: &str) -> Option<u32> {
    let digits = token.strip_prefix("section_")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A heading anchor (lowercase letters, digits, `-`, `_`) or `stage:<id>`.
fn valid_anchor(anchor: &str) -> bool {
    match anchor.strip_prefix("stage:") {
        Some(id) => {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        }
        None => {
            !anchor.is_empty()
                && anchor
                    .chars()
                    .all(|c| (c.is_alphanumeric() && !c.is_uppercase()) || matches!(c, '-' | '_'))
        }
    }
}
