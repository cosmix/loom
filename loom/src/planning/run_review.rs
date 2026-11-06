//! Reading the bullet sections of a run review written by `fs::plan_review`.

use crate::fs::plan_review::{
    OPEN_FINDINGS_HEADING, OPEN_QUESTIONS_HEADING, SPEC_CONFORMANCE_HEADING,
};

/// A run review section written in the shared bullet format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewSection {
    /// The spec conformance outcomes integration-verify recorded.
    SpecConformance,
    /// The questions recorded during the run.
    OpenQuestions,
    /// The suggestions never built and the entries deferred during the run.
    OpenFindings,
}

impl ReviewSection {
    /// The `## ...` line the section is written under.
    pub fn heading(self) -> &'static str {
        match self {
            Self::SpecConformance => SPEC_CONFORMANCE_HEADING,
            Self::OpenQuestions => OPEN_QUESTIONS_HEADING,
            Self::OpenFindings => OPEN_FINDINGS_HEADING,
        }
    }
}

/// One bullet of a run review section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewItem {
    /// The recording stage's name, unescaped.
    pub stage_name: String,
    /// The recording stage's id.
    pub stage_id: String,
    /// The recorded text, on one line.
    pub text: String,
}

/// The section's items in document order; empty when the section is absent or says it has none.
///
/// A section runs from its heading line to the next line starting `## `. Lines that are not
/// bullets in the shared format are skipped.
pub fn review_items(review: &str, section: ReviewSection) -> Vec<ReviewItem> {
    let heading = section.heading();
    let lines: Vec<&str> = review.lines().collect();
    // The writer emits the three sections last, so a line equal to the heading inside raw memory
    // text in an earlier section is skipped by starting from the last such line.
    let Some(start) = lines.iter().rposition(|line| line.trim_end() == heading) else {
        return Vec::new();
    };
    lines[start + 1..]
        .iter()
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| parse_item(line))
        .collect()
}

/// Parse ``- **<escaped name>** (`<id>`): <text>``.
fn parse_item(line: &str) -> Option<ReviewItem> {
    let rest = line.strip_prefix("- **")?;
    let (stage_name, rest) = split_name(rest)?;
    let rest = rest.strip_prefix(" (`")?;
    let id_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .unwrap_or(rest.len());
    let (stage_id, rest) = rest.split_at(id_len);
    if stage_id.is_empty() {
        return None;
    }
    let text = rest.strip_prefix("`):")?.trim();
    Some(ReviewItem {
        stage_name,
        stage_id: stage_id.to_owned(),
        text: text.to_owned(),
    })
}

/// Split an escaped name off at the first unescaped `**`, returning the unescaped name and the
/// text after the `**`. `\\` and `\*` are escapes; any other backslash is literal.
fn split_name(text: &str) -> Option<(String, &str)> {
    let mut name = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        match c {
            '\\' => match chars.peek() {
                Some(&(_, next @ ('\\' | '*'))) => {
                    name.push(next);
                    chars.next();
                }
                _ => name.push('\\'),
            },
            '*' if text[index + 1..].starts_with('*') => return Some((name, &text[index + 2..])),
            _ => name.push(c),
        }
    }
    None
}

#[cfg(test)]
mod tests;
