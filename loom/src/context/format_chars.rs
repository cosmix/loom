//! The Unicode format (Cf) characters, one table for every surface that strips or flattens them.

/// Unicode general category Cf (format), inclusive ranges, from the Unicode Character Database
/// `extracted/DerivedGeneralCategory.txt` (Unicode 15.1). Covers the bidirectional controls
/// U+061C, U+200E, U+200F, U+202A..U+202E and U+2066..U+2069.
const FORMAT_CHARS: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

/// Whether `c` is a Unicode format (Cf) character, among them the bidirectional controls.
pub(crate) fn is_format(c: char) -> bool {
    let code = u32::from(c);
    FORMAT_CHARS
        .iter()
        .any(|&(first, last)| (first..=last).contains(&code))
}

#[cfg(test)]
mod tests {
    use super::is_format;

    #[test]
    fn bidi_controls_tag_characters_and_joiners_are_format() {
        for ch in [
            '\u{202E}',
            '\u{2069}',
            '\u{206A}',
            '\u{E0041}',
            '\u{200B}',
            '\u{FEFF}',
        ] {
            assert!(is_format(ch), "{ch:?}");
        }
        assert!(!is_format('a') && !is_format(' ') && !is_format('\u{05D0}'));
    }
}
