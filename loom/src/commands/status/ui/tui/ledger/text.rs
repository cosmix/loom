//! Cell width and truncation helpers shared by the ledger's cell builders.

use ratatui::text::{Line, Span};

pub(super) fn padded(text: &str, width: u16) -> String {
    let mut value = truncate(text, usize::from(width));
    value.push_str(&" ".repeat(usize::from(width).saturating_sub(text_width(&value))));
    value
}

pub(super) fn truncate(text: &str, width: usize) -> String {
    if text_width(text) <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    // Reserve one cell for the ellipsis; a wide character that would straddle
    // the boundary is dropped rather than half-emitted.
    let budget = width - 1;
    let mut value = String::new();
    let mut used = 0;
    for character in text.chars() {
        let character_width = text_width(&character.to_string());
        if used + character_width > budget {
            break;
        }
        value.push(character);
        used += character_width;
    }
    value.push('…');
    value
}

/// Display width, not character count - unicode-width aware like `header::text_width`.
pub(super) fn text_width(text: &str) -> usize {
    Span::raw(text.to_owned()).width()
}

/// Cut `line` to `width` display cells, dropping any spans (or partial spans) past the
/// boundary. A span that straddles the boundary keeps its style but loses the characters
/// that would overflow.
pub(super) fn cut_line(line: Line<'static>, width: u16) -> Line<'static> {
    let mut remaining = width as usize;
    let mut spans = Vec::new();
    for span in line.spans {
        let mut text = String::new();
        for character in span.content.chars() {
            let character_width = text_width(&character.to_string());
            if character_width > remaining {
                break;
            }
            text.push(character);
            remaining = remaining.saturating_sub(character_width);
        }
        if !text.is_empty() {
            spans.push(Span::styled(text, span.style));
        }
        if remaining == 0 {
            break;
        }
    }
    Line::from(spans)
}

/// Total display width of a list of spans.
pub(super) fn spans_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(Span::width).sum()
}

#[cfg(test)]
mod tests {
    use super::{padded, truncate};

    #[test]
    fn truncation_counts_characters() {
        assert_eq!(truncate("áßçδ", 3), "áß…");
    }

    #[test]
    fn truncation_drops_a_wide_character_that_would_straddle_the_boundary() {
        // U+26A1 renders two cells wide; it must never be half-emitted.
        assert_eq!(truncate("⚡ conflict", 2), "…");
        assert_eq!(truncate("a⚡", 2), "a…");
    }

    #[test]
    fn padding_fills_the_requested_width() {
        assert_eq!(padded("x", 3), "x  ");
    }
}
