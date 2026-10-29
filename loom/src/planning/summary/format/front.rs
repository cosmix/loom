//! Front matter parse and emit.

use std::collections::BTreeMap;
use std::fmt::Write;

use serde::{Deserialize, Serialize};

use super::FormatError;
use crate::planning::summary::store;

/// The type of summary described by the front matter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryKind {
    /// A summary of one source document.
    Document,
    /// A summary of a collection of summaries.
    Overview,
}

/// Source and citation metadata stored ahead of a summary body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    /// Whether this summarizes a document or a collection.
    pub kind: SummaryKind,
    /// The source path or `overview/<feature>/{product,engineering}` id.
    pub of: String,
    /// The source document's hash, for document summaries.
    pub file: Option<String>,
    /// Source passages and their hashes.
    pub cites: BTreeMap<String, String>,
    /// Member summary IDs and their hashes.
    pub members: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFront {
    kind: SummaryKind,
    of: String,
    file: Option<String>,
    cites: BTreeMap<String, String>,
    members: Option<BTreeMap<String, String>>,
}

pub(crate) fn parse_front(yaml: &str, first_line: usize) -> Result<FrontMatter, Vec<FormatError>> {
    let raw: RawFront = serde_saphyr::from_str(yaml).map_err(|error| {
        let offset = error
            .location()
            .map_or(0, |loc| loc.line().saturating_sub(1) as usize);
        vec![FormatError {
            line: first_line + offset,
            message: error.to_string(),
        }]
    })?;
    let problem = match (&raw.kind, &raw.file, &raw.members) {
        (SummaryKind::Document, None, _) => Some("document front matter requires file"),
        (SummaryKind::Document, _, Some(_)) => {
            Some("document front matter must not contain members")
        }
        (SummaryKind::Overview, Some(_), _) => Some("overview front matter must not contain file"),
        (SummaryKind::Overview, _, None) => Some("overview front matter requires members"),
        (SummaryKind::Overview, _, _) if !store::is_overview(&raw.of) => {
            Some("overview front matter requires a valid feature overview id")
        }
        _ => None,
    };
    if let Some(message) = problem {
        return Err(vec![FormatError {
            line: first_line,
            message: message.to_owned(),
        }]);
    }
    Ok(FrontMatter {
        kind: raw.kind,
        of: raw.of,
        file: raw.file,
        cites: raw.cites,
        members: raw.members.unwrap_or_default(),
    })
}

pub(crate) fn render_front(front: &FrontMatter) -> String {
    let mut output = String::from("---\nkind: ");
    output.push_str(match front.kind {
        SummaryKind::Document => "document",
        SummaryKind::Overview => "overview",
    });
    output.push_str("\nof: ");
    output.push_str(&plain_or_quoted(&front.of));
    output.push('\n');
    if let Some(file) = &front.file {
        output.push_str("file: ");
        output.push_str(&plain_or_quoted(file));
        output.push('\n');
    }
    write_map(&mut output, "cites", &front.cites);
    if front.kind == SummaryKind::Overview {
        write_map(&mut output, "members", &front.members);
    }
    output.push_str("---\n");
    output
}

fn write_map(output: &mut String, name: &str, entries: &BTreeMap<String, String>) {
    output.push_str(name);
    if entries.is_empty() {
        output.push_str(": {}\n");
        return;
    }
    output.push_str(":\n");
    for (key, value) in entries {
        output.push_str("  ");
        output.push_str(&quoted(key));
        output.push_str(": ");
        output.push_str(&quoted(value));
        output.push('\n');
    }
}

fn plain_or_quoted(value: &str) -> String {
    let known_prefix = value.starts_with("doc/") || value.starts_with("sha256:");
    if (known_prefix || store::is_overview(value))
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ':'))
    {
        value.to_owned()
    } else {
        quoted(value)
    }
}

fn quoted(value: &str) -> String {
    let mut output = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(output, "\\u{:04x}", c as u32);
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}
