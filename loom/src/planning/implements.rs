//! Parsing plan `Implements:` declarations.

use crate::fs::knowledge::splice::fence_mask;
use crate::planning::kinds::{self, DocKind};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// The product sections a plan declares it implements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Implements {
    /// No declaration was found.
    Unmapped,
    /// The plan explicitly declares no product sections.
    NoneDeclared,
    /// The section numbers named by the declaration.
    Sections(Vec<u32>),
}

/// A declaration and any errors found while parsing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImplementsLine {
    /// The value of the first declaration.
    pub value: Implements,
    /// Zero-based line of the first declaration, if present.
    pub line: Option<usize>,
    /// Zero-based lines of subsequent declarations.
    pub duplicate_lines: Vec<usize>,
    /// Invalid tokens from the first declaration, trimmed as written.
    pub bad_tokens: Vec<String>,
}

/// Parse declarations before plan metadata, ignoring fenced code blocks.
pub fn parse(text: &str) -> ImplementsLine {
    let lines: Vec<&str> = text.lines().collect();
    let mask = fence_mask(&lines);
    let mut result = ImplementsLine {
        value: Implements::Unmapped,
        line: None,
        duplicate_lines: Vec::new(),
        bad_tokens: Vec::new(),
    };
    for (index, line) in lines.iter().enumerate() {
        if mask[index] {
            continue;
        }
        if line.contains("<!-- loom METADATA") {
            break;
        }
        if let Some(value) = line.strip_prefix("Implements:") {
            if result.line.is_some() {
                result.duplicate_lines.push(index);
            } else {
                result.line = Some(index);
                parse_value(value.trim(), &mut result);
            }
        }
    }
    result
}

/// The product section paths a feature plan declares it implements, in declaration order with
/// duplicates dropped.
///
/// `plan_rel` is the plan's repository-relative path. A standalone or unclassifiable plan,
/// `Implements: none` and a missing declaration yield nothing; invalid tokens are skipped (the
/// plan check reports them).
pub fn implemented_section_paths(plan_rel: &Path, text: &str) -> Vec<PathBuf> {
    let DocKind::Plan {
        feature: Some(feature),
        ..
    } = kinds::classify(plan_rel)
    else {
        return Vec::new();
    };
    let Implements::Sections(numbers) = parse(text).value else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for number in numbers {
        let path = kinds::section_path(&feature, number);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

fn parse_value(value: &str, result: &mut ImplementsLine) {
    if value.eq_ignore_ascii_case("none") {
        result.value = Implements::NoneDeclared;
        return;
    }
    let mut sections = Vec::new();
    for token in value.split(',').map(str::trim) {
        if let Some(number) = token_number(token) {
            sections.push(number);
        } else {
            result.bad_tokens.push(token.to_owned());
        }
    }
    result.value = Implements::Sections(sections);
}

fn token_number(token: &str) -> Option<u32> {
    if let Some(link) = token.strip_prefix('[') {
        let (label, target) = link.split_once("](")?;
        let target = target.strip_suffix(')')?;
        let stem = Path::new(target.split(['#', '?']).next()?).file_stem();
        return section_number(label).or_else(|| section_number(stem?.to_str()?));
    }
    section_number(token.strip_suffix(".md").unwrap_or(token))
}

fn section_number(token: &str) -> Option<u32> {
    let digits = token.strip_prefix("section_")?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}
