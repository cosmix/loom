//! Clock and line-packing helpers of the finished-plan screen.

use chrono::{DateTime, Local, Utc};
use ratatui::text::{Line, Span};

use super::ledger::text::text_width;

/// Spaces between two packed segments.
const SEGMENT_GAP: &str = "   ";

/// The local wall-clock time of `at`, `09:12`.
pub(super) fn clock(at: DateTime<Utc>) -> String {
    at.with_timezone(&Local).format("%H:%M").to_string()
}

/// Pack `segments` onto as few lines as fit `width`, left to right, each line led
/// by a one-cell margin. A segment wider than a line keeps a line to itself and is
/// cut by the caller.
pub(super) fn pack_segments(segments: Vec<Vec<Span<'static>>>, width: u16) -> Vec<Line<'static>> {
    let width = usize::from(width);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for segment in segments {
        let cells: usize = segment.iter().map(|span| text_width(&span.content)).sum();
        if !spans.is_empty() && used + SEGMENT_GAP.len() + cells + 1 > width {
            lines.push(Line::from(std::mem::take(&mut spans)));
        }
        if spans.is_empty() {
            spans.push(Span::raw(" "));
            used = 1;
        } else {
            spans.push(Span::raw(SEGMENT_GAP));
            used += SEGMENT_GAP.len();
        }
        used += cells;
        spans.extend(segment);
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

/// `text` cut from the left to `width` cells, keeping its tail behind a `…`.
pub(super) fn keep_tail(text: &str, width: usize) -> String {
    if text_width(text) <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1;
    for character in text.chars().rev() {
        let cells = text_width(&character.to_string());
        if used + cells > width {
            break;
        }
        used += cells;
        kept.push(character);
    }
    kept.push('…');
    kept.reverse();
    kept.into_iter().collect()
}
