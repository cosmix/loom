//! The `loom knowledge check --baseline` ratchet: recorded structural issues
//! are tolerated debt, anything else fails `--strict`.
//!
//! One line per structural issue, its [`baseline_key`]; `#` comments and blank
//! lines are ignored and a repeated entry is rejected, as in the
//! maintainability baseline. Unlike that baseline, drift is one-directional:
//! a recorded issue that no longer occurs only says the file can be
//! tightened.

use super::order::baseline_key;
use super::CatalogIssue;
use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::io::ErrorKind;
use std::path::Path;

/// Header written above the entries by [`render`].
const HEADER: &str = "\
# Knowledge check baseline: structural issues `loom knowledge check --strict --baseline` tolerates.
# One issue per line: <kind> <file> [<detail>]. Regenerate with `loom knowledge check --write-baseline <file>`.
";

/// A parsed baseline file.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CheckBaseline {
    entries: BTreeSet<String>,
}

/// Issues the baseline does not record, and recorded entries that no longer occur.
#[derive(Debug, Default)]
pub struct BaselineComparison<'a> {
    pub new: Vec<&'a CatalogIssue>,
    pub tightenable: Vec<String>,
}

impl CheckBaseline {
    /// Read a baseline file; a missing file is an empty baseline.
    pub fn read(path: &Path) -> Result<Self> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read baseline {}", path.display()))
            }
        };
        Self::parse(&source).or_else(|errors| {
            bail!(
                "Invalid knowledge check baseline {}:\n{}",
                path.display(),
                errors.join("\n")
            )
        })
    }

    /// Parse baseline text, collecting every malformed line rather than stopping at the first.
    pub fn parse(source: &str) -> std::result::Result<Self, Vec<String>> {
        let mut entries = BTreeSet::new();
        let mut errors = Vec::new();
        for (index, raw_line) in source.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if !entries.insert(line.to_string()) {
                errors.push(format!(
                    "line {}: duplicate baseline entry `{line}`",
                    index + 1
                ));
            }
        }
        errors.is_empty().then_some(Self { entries }).ok_or(errors)
    }

    /// Split the structural issues in `issues` into recorded and new, and list
    /// the recorded entries nothing matches any more.
    pub fn compare<'a>(&self, issues: &'a [CatalogIssue]) -> BaselineComparison<'a> {
        let mut seen = BTreeSet::new();
        let mut new = Vec::new();
        for issue in issues.iter().filter(|issue| !issue.is_review_only()) {
            let key = baseline_key(issue);
            if self.entries.contains(&key) {
                seen.insert(key);
            } else {
                new.push(issue);
            }
        }
        let tightenable = self.entries.difference(&seen).cloned().collect();
        BaselineComparison { new, tightenable }
    }
}

/// The baseline text recording every structural issue in `issues`, sorted.
pub fn render(issues: &[CatalogIssue]) -> String {
    let keys: BTreeSet<String> = issues
        .iter()
        .filter(|issue| !issue.is_review_only())
        .map(baseline_key)
        .collect();
    let mut text = HEADER.to_string();
    for key in keys {
        text.push_str(&key);
        text.push('\n');
    }
    text
}
