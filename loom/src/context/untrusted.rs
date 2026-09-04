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
//! This is the ONE definition all three surfaces call. A second copy would
//! duplicate a security rule that must never drift between them.

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
/// backticks become [`BACKTICK_SUBSTITUTE`], and the result is bounded. A value
/// with none of those is returned unchanged — the common case must not pay for
/// the hostile one.
pub(crate) fn inline_safe(value: &str) -> String {
    let flattened: String = value
        .chars()
        .map(|ch| match ch {
            '`' => BACKTICK_SUBSTITUTE,
            // `is_whitespace` covers U+2028/U+2029 as well as the ASCII set, so
            // no line-shaped character survives; `is_control` catches the rest,
            // including the ESC that would start an ANSI sequence.
            _ if ch.is_control() || ch.is_whitespace() => ' ',
            // Unicode category Cf (format characters) is neither control nor
            // whitespace, so it survives the two checks above untouched — and
            // it includes the bidi override/embedding controls (U+202A..U+202E,
            // U+2066..U+2069), zero-width and word-joining marks
            // (U+200B..U+200F, U+2060..U+2064), the Arabic letter mark
            // (U+061C), soft hyphen (U+00AD), and the byte-order mark
            // (U+FEFF). A value carrying e.g. U+202E (RIGHT-TO-LEFT OVERRIDE)
            // visually reverses everything rendered after it, so what a
            // reviewer reads on an agent-facing surface would not match what
            // was actually written. No crate dependency is added for this —
            // the ranges below are the specific code points this codebase's
            // untrusted sources are known to carry unvalidated.
            _ if matches!(
                ch,
                '\u{00AD}'
                    | '\u{061C}'
                    | '\u{200B}'..='\u{200F}'
                    | '\u{202A}'..='\u{202E}'
                    | '\u{2060}'..='\u{2064}'
                    | '\u{2066}'..='\u{2069}'
                    | '\u{FEFF}'
            ) =>
            {
                ' '
            }
            _ => ch,
        })
        .collect();
    let collapsed = flattened.split_whitespace().collect::<Vec<_>>().join(" ");
    crate::utils::truncate_for_display(&collapsed, MAX_INLINE_CHARS)
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
}
