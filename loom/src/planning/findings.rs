//! The findings ledger of a feature, `doc/plans/<feature>/findings.md`.
//!
//! The ledger is plain Markdown: a `# Findings` title, then one `## F-<n> <title>` entry per
//! finding with its `About:`, `Source:` and `Status:` lines, a blank line and one text paragraph.
//! `parse` reads it without the project, `entry_text` and `set_status` write it, and
//! `import_review` appends the open findings and questions of a finished run's review.
//!
//! Finding text is agent-written. Everything written passes the sanitizers in `text` so a finding
//! can never open a heading or a code fence of its own.

mod body;
mod import;
mod text;

#[cfg(test)]
mod tests;

use anyhow::{bail, Result};
use std::path::PathBuf;

use crate::planning::kinds;
use crate::planning::sections::sections_of;
use body::scan;

pub use import::import_review;
pub use text::{paragraph, reason, title};

/// The ledger's file name inside a feature's plan home.
pub const LEDGER_FILE: &str = "findings.md";

/// The problem code of a level-2 heading that is not `F-<n> <title>`.
pub const BAD_ENTRY: &str = "findings-bad-entry";
/// The problem code of an id used by more than one entry.
pub const DUPLICATE_ID: &str = "findings-duplicate-id";
/// The problem code of a missing or unknown `Status:` value.
pub const BAD_STATUS: &str = "findings-bad-status";

/// Where a finding stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingStatus {
    /// Not yet triaged.
    Open,
    /// Planned in the named plan, `PLAN-<slug>.md`.
    Planned(String),
    /// Folded into a metaplan entry, `metaplan#<entry anchor>`.
    Folded(String),
    /// Dismissed, with the reason.
    Dismissed(String),
}

/// One ledger entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// `F-<n>`.
    pub id: String,
    /// The heading text after the id.
    pub title: String,
    /// The `About:` tokens as written.
    pub about: Vec<String>,
    /// The `Source:` value, when present.
    pub source: Option<String>,
    /// The `Status:` value; open when missing or unknown.
    pub status: FindingStatus,
    /// The text paragraph.
    pub text: String,
}

/// A convention problem in the ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerProblem {
    /// One of [`BAD_ENTRY`], [`DUPLICATE_ID`] and [`BAD_STATUS`].
    pub code: &'static str,
    /// The 1-based line of the entry's heading.
    pub line: usize,
    /// One sentence naming the problem.
    pub message: String,
}

/// A parsed ledger.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    /// The valid entries in file order.
    pub findings: Vec<Finding>,
    /// What the parser skipped or defaulted.
    pub problems: Vec<LedgerProblem>,
}

impl FindingStatus {
    /// The text after `Status:`. Details pass the same sanitizers a write applies, so a status
    /// without a usable detail renders bare and reads back as a reported bad status.
    pub fn render(&self) -> String {
        match self {
            Self::Open => "open".to_owned(),
            Self::Planned(detail) => with_detail("planned", " ", detail),
            Self::Folded(detail) => with_detail("folded", " ", detail),
            Self::Dismissed(why) => {
                with_detail("dismissed", ": ", &reason(why).unwrap_or_default())
            }
        }
    }

    /// Whether the status carries a usable detail (`Open` has none to carry).
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Open => true,
            Self::Planned(detail) | Self::Folded(detail) => !text::one_line(detail).is_empty(),
            Self::Dismissed(why) => reason(why).is_some(),
        }
    }

    fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value == "open" {
            return Some(Self::Open);
        }
        if let Some(why) = value.strip_prefix("dismissed:") {
            return reason(why).map(Self::Dismissed);
        }
        let (word, detail) = value.split_once(' ')?;
        let detail = detail.trim();
        if detail.is_empty() {
            return None;
        }
        match word {
            "planned" => Some(Self::Planned(detail.to_owned())),
            "folded" => Some(Self::Folded(detail.to_owned())),
            _ => None,
        }
    }
}

fn with_detail(word: &str, separator: &str, detail: &str) -> String {
    let detail = text::one_line(detail);
    if detail.is_empty() {
        word.to_owned()
    } else {
        format!("{word}{separator}{detail}")
    }
}

/// The ledger's path inside a repository.
pub fn ledger_path(feature: &str) -> PathBuf {
    kinds::plans_dir(Some(feature)).join(LEDGER_FILE)
}

/// Read a ledger. Headings that are not `F-<n> <title>` are skipped and reported; fenced
/// headings are not headings at all.
pub fn parse(text: &str) -> Ledger {
    let doc = sections_of(text);
    let lines: Vec<&str> = text.lines().collect();
    let mut ledger = Ledger::default();
    for heading in doc.headings.iter().filter(|h| h.level == 2) {
        let line = heading.span.start_line + 1;
        let Some((id, title)) = split_heading(&heading.title) else {
            ledger.problems.push(LedgerProblem {
                code: BAD_ENTRY,
                line,
                message: format!(
                    "`## {}` is not an entry heading `## F-<n> <title>`",
                    heading.title
                ),
            });
            continue;
        };
        if ledger.findings.iter().any(|f| f.id == id) {
            ledger.problems.push(LedgerProblem {
                code: DUPLICATE_ID,
                line,
                message: format!("{id} is used by more than one entry"),
            });
            continue;
        }
        let end = heading.span.end_line.min(lines.len());
        let from = (heading.span.start_line + 1).min(end);
        let finding = read_entry(id, title, &lines[from..end], line, &mut ledger.problems);
        ledger.findings.push(finding);
    }
    ledger
}

fn read_entry(
    id: &str,
    title: &str,
    lines: &[&str],
    line: usize,
    problems: &mut Vec<LedgerProblem>,
) -> Finding {
    let body = scan(lines);
    let status = match body.status.map(|(_, value)| FindingStatus::parse(value)) {
        Some(Some(status)) => status,
        _ => {
            problems.push(LedgerProblem {
                code: BAD_STATUS,
                line,
                message: format!("{id} has a missing or unknown Status, read as open"),
            });
            FindingStatus::Open
        }
    };
    Finding {
        id: id.to_owned(),
        title: title.to_owned(),
        about: body.about.map(split_tokens).unwrap_or_default(),
        source: body.source.map(|s| s.trim().to_owned()),
        status,
        text: text::unescape(&lines[body.text_start..].join("\n")),
    }
}

fn split_tokens(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `(id, title)` of an entry heading `F-<n> <title>`; `n` is a positive integer without leading
/// zeros.
fn split_heading(heading: &str) -> Option<(&str, &str)> {
    let (id, title) = heading.split_once(' ')?;
    let title = title.trim();
    (id_number(id).is_some() && !title.is_empty()).then_some((id, title))
}

fn id_number(id: &str) -> Option<u64> {
    let digits = id.strip_prefix("F-")?;
    let number: u64 = digits.parse().ok()?;
    (number >= 1 && digits == number.to_string()).then_some(number)
}

/// The next free id: one past the highest `F-<n>` of the ledger.
pub fn next_id(ledger: &Ledger) -> String {
    format!("F-{}", next_number(ledger))
}

fn next_number(ledger: &Ledger) -> u64 {
    let highest = ledger
        .findings
        .iter()
        .filter_map(|f| id_number(&f.id))
        .max()
        .unwrap_or(0);
    highest + 1
}

/// One entry as it is written to the ledger, ending in a newline.
///
/// Title and text are sanitized here (see [`title`] and [`paragraph`]); `about` and `source`
/// are flattened to one line. An empty `about` writes no `About:` line.
pub fn entry_text(
    id: &str,
    title: &str,
    about: &[String],
    source: Option<&str>,
    status: &FindingStatus,
    text: &str,
) -> String {
    let mut out = format!("## {id} {}\n\n", self::title(title));
    if !about.is_empty() {
        let tokens: Vec<String> = about.iter().map(|t| text::one_line(t)).collect();
        out.push_str(&format!("About: {}\n", tokens.join(", ")));
    }
    if let Some(source) = source {
        out.push_str(&format!("Source: {}\n", text::one_line(source)));
    }
    out.push_str(&format!(
        "Status: {}\n\n{}\n",
        status.render(),
        paragraph(text)
    ));
    out
}

/// Rewrite the `Status:` line of entry `id` (inserting it when absent) and nothing else.
///
/// Errors on an unknown id and on a status without a usable detail.
pub fn set_status(ledger_text: &str, id: &str, status: &FindingStatus) -> Result<String> {
    if !status.is_valid() {
        bail!("the status {:?} has no usable detail", status.render());
    }
    let doc = sections_of(ledger_text);
    let Some(heading) = doc
        .headings
        .iter()
        .filter(|h| h.level == 2)
        .find(|h| split_heading(&h.title).is_some_and(|(found, _)| found == id))
    else {
        bail!("{id} is not an entry of the ledger");
    };
    let mut lines: Vec<String> = ledger_text
        .split_inclusive('\n')
        .map(str::to_owned)
        .collect();
    let start = heading.span.start_line;
    let end = heading.span.end_line.min(lines.len());
    let views: Vec<&str> = lines
        .iter()
        .map(|l| l.trim_end_matches(['\n', '\r']))
        .collect();
    let found = scan(&views[(start + 1).min(end)..end]);
    let edit = match (found.status, found.last_meta) {
        (Some((index, _)), _) => Edit::Replace(start + 1 + index),
        (None, Some(last)) => Edit::After(start + 1 + last),
        (None, None) => Edit::Fresh(start),
    };
    let status_line = format!("Status: {}", status.render());
    edit.apply(&mut lines, &status_line);
    Ok(lines.concat())
}

/// Where a `Status:` line goes.
enum Edit {
    /// Over the existing line at this index.
    Replace(usize),
    /// After the metadata line at this index.
    After(usize),
    /// An entry without metadata: under the heading at this index.
    Fresh(usize),
}

impl Edit {
    fn apply(self, lines: &mut Vec<String>, status_line: &str) {
        let is_blank = |line: Option<&String>| line.is_some_and(|l| l.trim().is_empty());
        match self {
            Self::Replace(index) => {
                let old = &lines[index];
                let ending = &old[old.trim_end_matches(['\n', '\r']).len()..];
                let replaced = format!("{status_line}{ending}");
                lines[index] = replaced;
            }
            Self::After(index) => insert(lines, index + 1, vec![format!("{status_line}\n")]),
            Self::Fresh(heading) => {
                let mut position = heading + 1;
                let mut added = Vec::new();
                if is_blank(lines.get(position)) {
                    position += 1;
                } else {
                    added.push("\n".to_owned());
                }
                added.push(format!("{status_line}\n"));
                if lines.get(position).is_some_and(|l| !l.trim().is_empty()) {
                    added.push("\n".to_owned());
                }
                insert(lines, position, added);
            }
        }
    }
}

/// Insert `added` at `position`, first ending the line above it when the file had no final
/// newline.
fn insert(lines: &mut Vec<String>, position: usize, added: Vec<String>) {
    if let Some(above) = position.checked_sub(1).and_then(|i| lines.get_mut(i)) {
        if !above.ends_with('\n') {
            above.push('\n');
        }
    }
    lines.splice(position..position, added);
}
