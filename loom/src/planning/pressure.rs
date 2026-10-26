//! Pressure-test records and open questions.

use crate::fs::knowledge::splice::fence_mask;
use crate::planning::sections::sections_of;
use serde::Serialize;

/// One dated pressure-test record in a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PressureRecord {
    /// Date in `YYYY-MM-DD` form.
    pub date: String,
    /// Zero-based line containing the record.
    pub line: usize,
    /// Record text following the date and colon.
    pub text: String,
}

/// Find dated list items inside level-two `Pressure tests` sections.
pub fn records(text: &str) -> Vec<PressureRecord> {
    let lines: Vec<_> = text.lines().collect();
    let mask = fence_mask(&lines);
    let sections = sections_of(text);
    let mut records = Vec::new();
    for heading in sections
        .headings
        .iter()
        .filter(|h| h.level == 2 && h.title == "Pressure tests")
    {
        for line in heading.span.start_line + 1..heading.span.end_line.min(lines.len()) {
            if !mask[line] {
                if let Some(record) = parse_record(lines[line], line) {
                    records.push(record);
                }
            }
        }
    }
    records
}

fn parse_record(text: &str, line: usize) -> Option<PressureRecord> {
    let rest = text.strip_prefix("- ")?;
    let date = rest.get(..10)?;
    let bytes = date.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return None;
    }
    let month = (bytes[5] - b'0') * 10 + bytes[6] - b'0';
    let day = (bytes[8] - b'0') * 10 + bytes[9] - b'0';
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let body = rest.get(10..)?.strip_prefix(':')?;
    Some(PressureRecord {
        date: date.to_owned(),
        line,
        text: body.trim().to_owned(),
    })
}

/// Count top-level list items under headings titled `Open questions`.
pub fn open_questions(text: &str) -> usize {
    let lines: Vec<_> = text.lines().collect();
    let mask = fence_mask(&lines);
    let sections = sections_of(text);
    let mut included = vec![false; lines.len()];
    for heading in sections
        .headings
        .iter()
        .filter(|h| h.title.eq_ignore_ascii_case("Open questions"))
    {
        let end = heading.span.end_line.min(lines.len());
        if let Some(span) = included.get_mut(heading.span.start_line + 1..end) {
            span.fill(true);
        }
    }
    lines
        .iter()
        .enumerate()
        .filter(|(i, line)| included[*i] && !mask[*i] && is_list_item(line))
        .count()
}

fn is_list_item(line: &str) -> bool {
    if line.starts_with("- ") || line.starts_with("* ") {
        return true;
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0
        && line
            .get(digits..)
            .is_some_and(|rest| rest.starts_with(". "))
}
