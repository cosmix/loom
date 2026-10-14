//! Word wrapping of plain text to a terminal width, for text a renderer must
//! show whole rather than cut.

use crate::commands::status::ui::tui::ledger::text::text_width;

/// Split `text` into lines of at most `width` display cells.
///
/// The text's own line breaks stay, a blank line included. Words are separated
/// by single spaces on the wrapped lines, and a word wider than `width` is
/// broken at the width instead of overflowing. A `width` of 0 counts as 1.
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    text.lines()
        .flat_map(|line| wrap_line(line, width))
        .collect()
}

fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in line.split_whitespace() {
        for piece in break_word(word, width) {
            let gap = usize::from(!current.is_empty());
            if !current.is_empty() && text_width(&current) + gap + text_width(&piece) > width {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(&piece);
        }
    }
    lines.push(current);
    lines
}

/// `word` cut into pieces of at most `width` cells.
fn break_word(word: &str, width: usize) -> Vec<String> {
    if text_width(word) <= width {
        return vec![word.to_owned()];
    }
    let mut pieces = Vec::new();
    let mut piece = String::new();
    for character in word.chars() {
        let cells = text_width(&character.to_string());
        if !piece.is_empty() && text_width(&piece) + cells > width {
            pieces.push(std::mem::take(&mut piece));
        }
        piece.push(character);
    }
    pieces.push(piece);
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_that_fits_is_one_line() {
        assert_eq!(wrap_text("run the stage", 20), ["run the stage"]);
    }

    #[test]
    fn text_wraps_at_word_boundaries_within_the_width() {
        assert_eq!(
            wrap_text("one two three four five", 9),
            ["one two", "three", "four five"]
        );
    }

    #[test]
    fn the_texts_own_line_breaks_and_blank_lines_stay() {
        assert_eq!(
            wrap_text("first line\n\nsecond line", 40),
            ["first line", "", "second line"]
        );
    }

    #[test]
    fn a_word_wider_than_the_width_is_broken() {
        assert_eq!(wrap_text("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wide_characters_count_two_cells() {
        assert_eq!(wrap_text("日本語 日本語", 6), ["日本語", "日本語"]);
    }

    #[test]
    fn nothing_wraps_to_no_lines() {
        assert!(wrap_text("", 10).is_empty());
    }
}
