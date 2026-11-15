use crate::context::format_chars::is_format;

/// `text` on one line: control characters and whitespace runs become single
/// spaces, and Unicode format (Cf) characters are dropped, so agent-authored
/// text can neither span journal lines, steer a terminal, nor read reordered
/// (a U+202E bidi override) or hide a zero-width character. Every renderer of
/// a review value goes through this.
pub fn single_line(text: &str) -> String {
    let visible: String = text.chars().filter(|&c| !is_format(c)).collect();
    visible
        .split(|c: char| c.is_whitespace() || c.is_control())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::single_line;

    #[test]
    fn single_line_flattens_control_characters_and_whitespace() {
        assert_eq!(single_line(" a\n\tb\u{1b}[31m  c \r\n"), "a b [31m c");
    }

    #[test]
    fn single_line_drops_bidi_overrides_and_zero_width_characters() {
        assert_eq!(
            single_line("ok\u{202E}gnp.exe \u{200B}\u{FEFF} a\u{2066}b\u{E0041}"),
            "okgnp.exe ab"
        );
    }
}
