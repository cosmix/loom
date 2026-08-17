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
//! This is the ONE definition both surfaces call. A second copy would
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
}
