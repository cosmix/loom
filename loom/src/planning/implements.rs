//! Parsing plan `Implements:` declarations.

use crate::fs::knowledge::splice::fence_mask;
use crate::planning::kinds::{self, DocKind};
use crate::planning::trace::tokens::{section_refs, SectionRef};
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
    /// The anchored tokens of the first declaration as `(section number, anchor)`, in
    /// declaration order and deduplicated. A section named both with and without an anchor is
    /// still implemented whole: see `whole`.
    pub anchors: Vec<(u32, String)>,
    /// The sections of the first declaration implemented whole, in first-seen order: those named
    /// by at least one token without an anchor. A section named only through anchors is absent.
    pub whole: Vec<u32>,
}

impl ImplementsLine {
    /// The claims the first declaration makes, in section order: a `SectionRef` without an anchor
    /// for each whole section, otherwise one per anchored token of that section.
    pub fn claims(&self) -> Vec<SectionRef> {
        let Implements::Sections(numbers) = &self.value else {
            return Vec::new();
        };
        let mut refs = Vec::new();
        for &number in numbers {
            if self.whole.contains(&number) {
                refs.push(SectionRef {
                    number,
                    anchor: None,
                });
                continue;
            }
            let anchors = self.anchors.iter().filter(|(own, _)| *own == number);
            for (_, anchor) in anchors {
                refs.push(SectionRef {
                    number,
                    anchor: Some(anchor.clone()),
                });
            }
        }
        refs
    }
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
        anchors: Vec::new(),
        whole: Vec::new(),
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
    let (refs, bad) = section_refs(value);
    let mut numbers = Vec::new();
    for section in refs {
        if !numbers.contains(&section.number) {
            numbers.push(section.number);
        }
        match section.anchor {
            Some(anchor) => result.anchors.push((section.number, anchor)),
            None if !result.whole.contains(&section.number) => result.whole.push(section.number),
            None => {}
        }
    }
    result.bad_tokens = bad;
    result.value = Implements::Sections(numbers);
}
