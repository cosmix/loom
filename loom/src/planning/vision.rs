//! The product vision's specification sections table.

use std::collections::HashSet;

use serde::Serialize;

use crate::fs::knowledge::splice::fence_mask;
use crate::planning::sections::sections_of;

/// A numbered section listed in the vision, in table order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VisionRow {
    /// Section number.
    pub number: u32,
    /// Text of the second table cell.
    pub name: String,
    /// Table row line, 0-based.
    pub line: usize,
}

/// Why the vision's section list could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VisionProblem {
    /// No matching heading exists outside fenced code.
    NoHeading,
    /// The matching heading contains no table.
    NoTable {
        /// Heading line, 0-based.
        heading_line: usize,
    },
    /// The first table has body rows and none of them is numbered.
    NoNumberedRows {
        /// Heading line, 0-based.
        heading_line: usize,
    },
}

/// Parse the first table under the first `Specification sections` heading. A table with a
/// header and delimiter row but no body rows yields `Ok(vec![])`: the vision lists no sections.
pub fn parse_sections_table(text: &str) -> Result<Vec<VisionRow>, VisionProblem> {
    let sections = sections_of(text);
    let heading = sections
        .headings
        .iter()
        .find(|heading| heading.title.eq_ignore_ascii_case("Specification sections"))
        .ok_or(VisionProblem::NoHeading)?;
    let heading_line = heading.span.start_line;
    let lines: Vec<_> = text.lines().collect();
    let mask = fence_mask(&lines);
    let end = heading.span.end_line;
    let table = (heading_line + 1..end.saturating_sub(1))
        .find(|&line| {
            !mask[line] && !mask[line + 1] && is_table_header(lines[line], lines[line + 1])
        })
        .ok_or(VisionProblem::NoTable { heading_line })?;

    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    let mut body_rows = 0;
    for line in table + 2..end {
        if mask[line] {
            break;
        }
        let Some(cells) = table_cells(lines[line]) else {
            break;
        };
        body_rows += 1;
        if let Some(row) = numbered_row(&cells, line) {
            if seen.insert(row.number) {
                rows.push(row);
            }
        }
    }
    // A table with no body rows is the file-expressed "no product spec" (section 01, "Skipped
    // and reordered steps"); body rows that are none of them numbered are an unreadable table.
    if rows.is_empty() && body_rows > 0 {
        Err(VisionProblem::NoNumberedRows { heading_line })
    } else {
        Ok(rows)
    }
}

fn is_table_header(header: &str, delimiter: &str) -> bool {
    let (Some(header), Some(delimiter)) = (table_cells(header), table_cells(delimiter)) else {
        return false;
    };
    header.len() >= 2
        && header.len() == delimiter.len()
        && delimiter.iter().all(|cell| {
            let dashes = cell.trim().trim_matches(':');
            !dashes.is_empty() && dashes.bytes().all(|byte| byte == b'-')
        })
}

fn table_cells(line: &str) -> Option<Vec<&str>> {
    let line = line.trim();
    if !line.contains('|') {
        return None;
    }
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = line.strip_suffix('|').unwrap_or(line);
    Some(line.split('|').collect())
}

fn numbered_row(cells: &[&str], line: usize) -> Option<VisionRow> {
    let number = cells.first()?.trim().trim_matches('`').trim();
    if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(VisionRow {
        number: number.parse().ok()?,
        name: cells.get(1)?.trim().to_string(),
        line,
    })
}
