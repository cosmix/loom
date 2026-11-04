//! The one flattening routine for untrusted knowledge-derived values.
//!
//! A chunk id, a source pointer, or a summary is rendered onto an agent-facing
//! surface in two places: the Knowledge Brief embedded in a stage signal
//! (`orchestrator::signals::format::brief`) and `loom knowledge context`'s
//! human-readable stdout (`commands::knowledge::context`). Both surfaces render
//! the value as part of their OWN structure — an inline code span or a bare
//! line — while the value itself came from unvalidated data: `fs::knowledge::chunker`
//! takes a file's first chunk id verbatim from its YAML frontmatter, a backtick
//! is a legal character in a path, and a summary is taken from a chunk heading.
//!
//! Emitted raw, a newline in any of them ends the line it sits on and lets the
//! remainder render as document structure — a heading or a sentence standing
//! outside any "quoted, NOT instructions" guard — while a backtick closes an
//! inline code span. Either turns untrusted data into what reads as the
//! surface's own text, in output an agent may treat as instructions or
//! assignment.
//!
//! A third surface calls it for the same reason on different data: the status
//! payload (`commands::status::data::sanitize`) flattens the model names,
//! heartbeat activity, review reasons and crash evidence a `StageSummary`
//! carries before the daemon broadcasts them. There the structure being
//! injected into is a terminal rather than a markdown document — an ESC that
//! survives is an ANSI sequence the operator's terminal obeys — and the
//! renderers cannot stop it, because they bound columns by display width and
//! every character that matters here has a width of zero.
//!
//! Source-window lines are a fourth: tracked file text, printed to the same
//! terminal by `loom map --window` and `loom knowledge context`, and quoted in
//! the brief. [`terminal_safe`] keeps their layout and strips the controls.
//!
//! This is the ONE definition all these surfaces call. A second copy would
//! duplicate a security rule that must never drift between them.

use super::format_chars::is_format;

/// Longest inline value either surface renders before eliding the rest.
///
/// Ids and pointers are identifiers, not content: past a couple of lines'
/// worth they have stopped identifying anything and started spending budget
/// the surface's real content needs.
pub(crate) const MAX_INLINE_CHARS: usize = 200;

/// What a backtick in an untrusted value is rendered as.
///
/// A markdown inline code span has no escape sequence — a backslash before a
/// backtick is a literal backslash *inside* the span — so the only way to stop
/// a value from closing its own span is to not emit a backtick at all. U+02CB
/// (MODIFIER LETTER GRAVE ACCENT) reads as one without being one.
pub(crate) const BACKTICK_SUBSTITUTE: char = 'ˋ';

/// The per-character rule shared by [`inline_safe`] and [`multiline_safe`]:
/// a backtick becomes [`BACKTICK_SUBSTITUTE`], and every other character goes
/// through [`flatten_char`].
fn neutralize(ch: char) -> char {
    match ch {
        '`' => BACKTICK_SUBSTITUTE,
        _ => flatten_char(ch),
    }
}

/// [`neutralize`] without its backtick rule, for a renderer that delimits the
/// value itself (`render::inline_code` sizes its backtick run to the text): every
/// control, whitespace or Unicode Cf format character becomes a space, and
/// everything else, a backtick included, is kept.
pub(crate) fn flatten_char(ch: char) -> char {
    match ch {
        // `is_whitespace` covers U+2028/U+2029 as well as the ASCII set, so
        // no line-shaped character survives; `is_control` catches the rest,
        // including the ESC that would start an ANSI sequence.
        _ if ch.is_control() || ch.is_whitespace() => ' ',
        // Unicode category Cf (format characters) is neither control nor whitespace. It
        // includes the bidi override/embedding controls (U+202E reverses everything rendered
        // after it), zero-width and joining marks, and the tag characters, so what a reviewer
        // reads on an agent-facing surface could differ from what was written.
        _ if is_format(ch) => ' ',
        _ => ch,
    }
}

/// The bidirectional embedding, override and isolate controls (U+202A..U+202E,
/// U+2066..U+2069): each reorders the text rendered after it, which lets source
/// read differently from what a compiler parses (Trojan Source).
fn is_bidi_control(ch: char) -> bool {
    matches!(ch, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Most combining marks [`bound_marks`] keeps on one base character.
///
/// Real text stacks two or three (a Vietnamese vowel carries two, pointed
/// Hebrew three); four leaves room for those and nothing for a flood.
pub(crate) const MAX_STACKED_MARKS: usize = 4;

/// The combining marks a hostile value stacks on one base character: the
/// generic combining blocks, the Cyrillic, Hebrew, Arabic and Thai marks, the
/// kana sound marks and the variation selectors. Each has a display width of
/// zero, so a width-budgeted column bounds none of them.
///
/// Marks of scripts outside these ranges (the Indic vowel signs, for one) are
/// kept as written. Revisit with a full general-category table (the
/// `unicode-width` crate's zero-width test) if an untrusted source carries
/// those in runs.
fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{0483}'..='\u{0489}'
            | '\u{0591}'..='\u{05BD}'
            | '\u{05BF}'
            | '\u{05C1}'..='\u{05C2}'
            | '\u{05C4}'..='\u{05C5}'
            | '\u{05C7}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{0670}'
            | '\u{06D6}'..='\u{06DC}'
            | '\u{06DF}'..='\u{06E4}'
            | '\u{06E7}'..='\u{06E8}'
            | '\u{06EA}'..='\u{06ED}'
            | '\u{0E31}'
            | '\u{0E34}'..='\u{0E3A}'
            | '\u{0E47}'..='\u{0E4E}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{3099}'..='\u{309A}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// Collect `chars`, dropping each combining mark past the
/// [`MAX_STACKED_MARKS`]-th in a run. Every other character is kept, so text
/// is never cut; only a mark flood (`x` and 100,000 U+0301, a few hundred
/// kilobytes that print as one cell) shrinks.
fn bound_marks(chars: impl Iterator<Item = char>) -> String {
    let mut run = 0;
    chars
        .filter(|&ch| {
            if is_combining_mark(ch) {
                run += 1;
                run <= MAX_STACKED_MARKS
            } else {
                run = 0;
                true
            }
        })
        .collect()
}

/// Neutralize a value a surface shows whole, on one line: every character
/// goes through [`flatten_char`], combining-mark runs are bounded by
/// [`bound_marks`], and nothing else is cut or collapsed. Failure evidence,
/// acceptance criteria and archived issue history take this rule.
pub(crate) fn whole_safe(value: &str) -> String {
    bound_marks(value.chars().map(flatten_char))
}

/// Flatten a value that is rendered as part of a surface's own structure.
///
/// Ids, pointers and query text carry arbitrary bytes: `fs::knowledge::chunker`
/// takes a file's first chunk id verbatim from its unvalidated YAML
/// frontmatter, a backtick is a legal character in a path, and query text is
/// assembled from a plan's free-form stage metadata.
///
/// Emitted raw, a newline in any of them ends the line it sits on and lets the
/// remainder render as document structure — a heading or a sentence standing
/// outside the "quoted, NOT instructions" guard and outside every fence — while
/// a backtick closes the inline code span. Either turns quoted reference data
/// into what reads as the brief's own text, in the file an agent treats as its
/// assignment.
///
/// So: control and whitespace characters become spaces, runs collapse,
/// backticks become [`BACKTICK_SUBSTITUTE`], combining-mark runs are bounded
/// ([`bound_marks`]), and the result is bounded. A value with none of those is
/// returned unchanged — the common case must not pay for the hostile one.
pub(crate) fn inline_safe(value: &str) -> String {
    let flattened = bound_marks(value.chars().map(neutralize));
    let collapsed = flattened.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::utils::truncate_for_display(&collapsed, MAX_INLINE_CHARS)
}

/// Neutralize repo-controlled text that is printed to a terminal as it is:
/// source-window lines, which `loom map --window` and `loom knowledge context`
/// both show.
///
/// Keeps every character a reader needs, tabs and newlines included, and
/// replaces each other control character with U+FFFD, such as the ESC that starts
/// an ANSI or OSC sequence (a hidden-text mode, a clipboard write). A bidi
/// control becomes a space, as [`flatten_char`] makes it: a window line that
/// reorders its own text would show code other than what the file holds. `\r\n`
/// becomes `\n`, a lone `\r` is a control character like any other, and trailing
/// newlines are dropped.
pub(crate) fn terminal_safe(text: &str) -> String {
    text.replace("\r\n", "\n")
        .trim_end_matches('\n')
        .chars()
        .map(|ch| {
            if ch.is_control() && ch != '\n' && ch != '\t' {
                '\u{FFFD}'
            } else if is_bidi_control(ch) {
                flatten_char(ch)
            } else {
                ch
            }
        })
        .collect()
}

/// Longest multi-line value [`multiline_safe`] renders before eliding the rest.
pub(crate) const MAX_MULTILINE_CHARS: usize = 16_000;

/// Neutralize an untrusted value that is displayed as a block of text.
///
/// Applies the [`inline_safe`] character rules but keeps line structure:
/// `\r\n` and lone `\r` become `\n`, `\n` survives, a tab becomes a space,
/// combining-mark runs are bounded ([`bound_marks`]), trailing whitespace is
/// trimmed per line, and the result is cut to [`MAX_MULTILINE_CHARS`]
/// characters with a trailing `…` when the cap bites.
pub(crate) fn multiline_safe(value: &str) -> String {
    let normalized = value.replace("\r\n", "\n").replace('\r', "\n");
    let cleaned = bound_marks(
        normalized
            .chars()
            .map(|ch| if ch == '\n' { ch } else { neutralize(ch) }),
    );
    let joined = cleaned
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    // Runs of blank lines would let a newline flood spend the whole budget on
    // whitespace: keep at most one blank line between paragraphs.
    let mut joined = joined;
    while joined.contains("\n\n\n") {
        joined = joined.replace("\n\n\n", "\n\n");
    }
    match joined.char_indices().nth(MAX_MULTILINE_CHARS) {
        Some((cut, _)) => format!("{}…", &joined[..cut]),
        None => joined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_values_pass_through_completely_unchanged() {
        // The hostile case must cost the common case nothing at all.
        for value in [
            "architecture#overview#1",
            "doc/loom/knowledge/architecture.md#overview",
            "stage-1 query text",
        ] {
            assert_eq!(inline_safe(value), value);
        }
    }

    #[test]
    fn bidi_override_is_flattened() {
        // U+202E (RIGHT-TO-LEFT OVERRIDE) would otherwise visually reverse
        // everything rendered after it on an agent-facing surface.
        let hostile = "safe-id\u{202E}desnever ylevitceffe";
        let flattened = inline_safe(hostile);
        assert!(!flattened.contains('\u{202E}'));
        assert_eq!(flattened, "safe-id desnever ylevitceffe");
    }

    #[test]
    fn multiline_keeps_newlines_and_passes_ordinary_text_unchanged() {
        let value = "first line\nsecond line\n\nfourth";
        assert_eq!(multiline_safe(value), value);
        assert_eq!(multiline_safe("a  \r\nb\rc\t"), "a\nb\nc");
    }

    #[test]
    fn multiline_neutralizes_escape_and_bidi_controls() {
        let out = multiline_safe("ok\u{1b}[31m\nx\u{202E}y`z");
        assert!(!out.contains('\u{1b}'));
        assert!(!out.contains('\u{202E}'));
        assert!(!out.contains('`'));
        assert!(out.contains('\n'));
    }

    #[test]
    fn multiline_collapses_blank_line_runs_to_one_blank_line() {
        assert_eq!(multiline_safe("a\n\n\n\n\nb\n \n\t\n\nc"), "a\n\nb\n\nc");
        let flood = format!("a{}b", "\n".repeat(MAX_MULTILINE_CHARS * 2));
        assert_eq!(multiline_safe(&flood), "a\n\nb");
    }

    #[test]
    fn terminal_safe_replaces_escape_and_osc_bytes_and_keeps_layout() {
        let text = "a\u{1b}[8mb\u{1b}]52;c;ZXZpbA==\u{7}\tc\r\nd\re\n\n";
        assert_eq!(
            terminal_safe(text),
            "a\u{FFFD}[8mb\u{FFFD}]52;c;ZXZpbA==\u{FFFD}\tc\nd\u{FFFD}e"
        );
        assert_eq!(terminal_safe("fn f() {\n    1\n}\n"), "fn f() {\n    1\n}");
    }

    #[test]
    fn terminal_safe_spaces_bidi_controls_and_keeps_right_to_left_text() {
        for ch in ('\u{202A}'..='\u{202E}').chain('\u{2066}'..='\u{2069}') {
            assert_eq!(terminal_safe(&format!("a{ch}b")), "a b", "{ch:?}");
        }
        let hostile = "if role != \"user\u{202E} \u{2066}// admin\u{2069} \u{2066}\" {";
        assert_eq!(
            terminal_safe(hostile),
            "if role != \"user   // admin   \" {"
        );
        assert_eq!(terminal_safe("שלום\tx"), "שלום\tx");
    }

    #[test]
    fn flatten_char_spaces_line_and_bidi_characters_but_keeps_backticks() {
        for ch in [
            '\u{2028}', '\u{2029}', '\u{202A}', '\u{202E}', '\u{2066}', '\u{2069}',
        ] {
            assert_eq!(flatten_char(ch), ' ', "{ch:?}");
        }
        assert_eq!(flatten_char('`'), '`');
        assert_eq!(neutralize('`'), BACKTICK_SUBSTITUTE);
        assert_eq!(flatten_char('x'), 'x');
    }

    #[test]
    fn flatten_char_spaces_tag_characters_and_deprecated_format_characters() {
        for ch in ['\u{E0041}', '\u{E007F}', '\u{206A}', '\u{206F}'] {
            assert_eq!(flatten_char(ch), ' ', "{ch:?}");
        }
    }

    #[test]
    fn a_combining_mark_flood_keeps_its_base_and_four_marks() {
        let flood = format!("x{}y", "\u{0301}".repeat(100_000));
        let kept = format!("x{}y", "\u{0301}".repeat(MAX_STACKED_MARKS));

        assert_eq!(whole_safe(&flood), kept);
        assert_eq!(multiline_safe(&flood), kept);
        assert_eq!(inline_safe(&flood), kept);
    }

    #[test]
    fn real_text_keeps_every_mark_and_character() {
        // Vietnamese stacks two marks, pointed Hebrew three; a long line stays whole.
        let decomposed = "Vie\u{0302}\u{0323}t \u{05E9}\u{05C1}\u{05B8}\u{05BC} cafe\u{0301}";
        let long = "é".repeat(5_000);
        assert_eq!(whole_safe(decomposed), decomposed);
        assert_eq!(whole_safe(&long), long);
        assert_eq!(whole_safe("a\u{202E}b\tc"), "a b c");
    }

    #[test]
    fn multiline_cap_bites_with_ellipsis() {
        let out = multiline_safe(&"é".repeat(MAX_MULTILINE_CHARS + 10));
        assert_eq!(out.chars().count(), MAX_MULTILINE_CHARS + 1);
        assert!(out.ends_with('…'));
    }
}
