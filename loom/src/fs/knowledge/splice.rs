//! Level-agnostic `#{2,6} <heading>` section splicing for knowledge files.
//!
//! Extracted from `dir.rs` (loom-bugs.txt BUG 2): the original splicer only
//! matched an exact `## <heading>` line, so a heading nested as `### ` (or
//! deeper) under a group heading was unreachable — `replace_section` reported
//! it as absent and appended a disconnected duplicate at EOF instead of
//! correcting it in place. This module matches ANY heading level 2-6,
//! replaces up to the next heading of the same or shallower depth (so a
//! deeper nested heading stays inside the replaced section), and ignores
//! `#`-prefixed lines inside fenced code blocks.

/// What `splice_section` actually did, so callers can report accurately
/// instead of assuming "found" from a separate, possibly-stale pre-check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionOutcome {
    /// The heading was found at this ATX level (2-6) and its section body
    /// was replaced in place.
    Replaced { level: usize },
    /// No heading matched; a new `## <heading>` section was appended at EOF.
    Appended,
}

/// Splice `content` into the first heading (any level 2-6) matching
/// `heading` in `base`, replacing everything up to (but excluding) the next
/// heading of the same or shallower level, or EOF. Appends a new `##
/// <heading>` section at EOF when no heading matches and all fences close.
/// An unmatched fence rejects the edit. Shared by
/// [`super::dir::KnowledgeDir::replace_section`] and
/// [`super::dir::KnowledgeDir::replace_section_target`].
pub(crate) fn splice_section(
    base: String,
    heading: &str,
    content: &str,
) -> Result<(String, SectionOutcome), SectionEditError> {
    let lines: Vec<&str> = base.lines().collect();
    Ok(match find_section(&lines, heading)? {
        Some(SectionSpan { start, end, level }) => {
            let replaced = assemble_replacement(&base, &lines, start, end, content);
            (replaced, SectionOutcome::Replaced { level })
        }
        None => (
            append_new_section(base, heading, content),
            SectionOutcome::Appended,
        ),
    })
}

/// Why a section edit that requires an existing heading did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionEditError {
    /// No heading at any level 2-6 matched.
    NotFound,
    /// The heading matched at this level, but the edit only applies to `## `
    /// sections (the unit the chunker splits on).
    NotLevelTwo { level: usize },
    /// The document contains an unmatched fence, so its edit span is ambiguous.
    UnclosedFence,
}

/// Remove the first heading (any level 2-6) matching `heading` together with
/// its body and nested deeper headings, using the same span rules as
/// [`splice_section`]. Returns the new text and the level that matched, or
/// `None` when no heading matches; an unmatched fence rejects the edit.
pub(crate) fn delete_section(
    base: &str,
    heading: &str,
) -> Result<Option<(String, usize)>, SectionEditError> {
    let lines: Vec<&str> = base.lines().collect();
    let Some(SectionSpan { start, end, level }) = find_section(&lines, heading)? else {
        return Ok(None);
    };
    let mut kept: Vec<&str> = lines[..start].to_vec();
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    if !kept.is_empty() && end < lines.len() {
        kept.push("");
    }
    kept.extend_from_slice(&lines[end..]);
    Ok(Some((join_lines(&kept, base), level)))
}

/// Write `marker` as the first line under the `## <heading>` section,
/// replacing a state marker already sitting there (the first non-blank line
/// under the heading, as the chunker reads it).
pub(crate) fn set_section_marker(
    base: &str,
    heading: &str,
    marker: &str,
) -> Result<String, SectionEditError> {
    let lines: Vec<&str> = base.lines().collect();
    let span = find_section(&lines, heading)?.ok_or(SectionEditError::NotFound)?;
    if span.level != 2 {
        return Err(SectionEditError::NotLevelTwo { level: span.level });
    }
    let mut edited = lines.clone();
    let existing = (span.start + 1..span.end)
        .find(|&i| !lines[i].trim().is_empty())
        .filter(|&i| super::chunker::state_marker_of(lines[i]).is_some());
    match existing {
        Some(i) => edited[i] = marker,
        None => edited.insert(span.start + 1, marker),
    }
    Ok(join_lines(&edited, base))
}

/// Line range `[start, end)` and ATX level of a matched section.
struct SectionSpan {
    start: usize,
    end: usize,
    level: usize,
}

/// The first non-fenced heading (level 2-6) whose text is exactly `heading`,
/// spanning up to the next heading of the same or shallower level, or EOF.
fn find_section(lines: &[&str], heading: &str) -> Result<Option<SectionSpan>, SectionEditError> {
    let (in_fence, unclosed) = scan_fences(lines);
    if unclosed {
        return Err(SectionEditError::UnclosedFence);
    }
    let Some((start, level)) = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !in_fence[*i])
        .find_map(|(i, line)| heading_level_of(line, heading).map(|level| (i, level)))
    else {
        return Ok(None);
    };
    let end = section_end(lines, &in_fence, start, level);
    Ok(Some(SectionSpan { start, end, level }))
}

/// Join `lines` with `\n`, ending with a newline when `base` did.
fn join_lines(lines: &[&str], base: &str) -> String {
    let mut result = lines.join("\n");
    if !result.is_empty() && base.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// If `line` (trailing whitespace ignored) is an ATX heading, returns its
/// level (1-6) and the heading text after the hashes and required
/// whitespace. A bare `#`/`##`/... line with no text yields an empty string.
/// `None` for a non-heading line, including `###Foo` (no space - not a valid
/// ATX heading) and a run of 7+ hashes (past the ATX heading limit).
pub(crate) fn atx_heading(line: &str) -> Option<(usize, &str)> {
    super::scanner::atx_heading(line)
}

/// `text` (as `atx_heading` returns it) without an ATX closing sequence: a trailing run of `#`
/// that is the whole text or is preceded by a space or tab (`Title ##` becomes `Title`, `C#` is
/// unchanged).
pub(crate) fn strip_closing_sequence(text: &str) -> &str {
    let open = text.trim_end_matches('#');
    if open.len() == text.len() {
        return text;
    }
    if open.is_empty() {
        return "";
    }
    if open.ends_with([' ', '\t']) {
        open.trim_end_matches([' ', '\t'])
    } else {
        text
    }
}

/// Level (2-6) of `line` if it is an ATX heading whose text exactly matches
/// `heading`. `None` for anything else, including an H1 (`replace_section`
/// only ever targets sub-sections, never the document title).
fn heading_level_of(line: &str, heading: &str) -> Option<usize> {
    let (level, text) = atx_heading(line)?;
    if (2..=6).contains(&level) && text == heading {
        Some(level)
    } else {
        None
    }
}

/// Index of the first non-fenced line after `start` whose heading level is
/// the same as or shallower than `level` (the end of the matched section), or
/// `lines.len()` if none exists (the section runs to EOF).
pub(crate) fn section_end(lines: &[&str], in_fence: &[bool], start: usize, level: usize) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(i, line)| !in_fence[*i] && atx_heading(line).is_some_and(|(l, _)| l <= level))
        .map(|(i, _)| i)
        .unwrap_or(lines.len())
}

/// Build the replacement text: everything before `start`, the heading at its
/// original `level` with `content` as its body, then everything from `end`
/// onward (preserving the original trailing-newline shape).
fn assemble_replacement(
    base: &str,
    lines: &[&str],
    start: usize,
    end: usize,
    content: &str,
) -> String {
    let mut result = String::new();
    for line in &lines[..start] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!("{}\n\n{content}\n", lines[start]));
    if end < lines.len() {
        result.push('\n');
        for (i, line) in lines[end..].iter().enumerate() {
            result.push_str(line);
            if i < lines.len() - end - 1 {
                result.push('\n');
            }
        }
        if base.ends_with('\n') {
            result.push('\n');
        }
    }
    result
}

/// No heading matched `heading` anywhere: append a brand-new `## <heading>`
/// section at EOF, exactly as the original (H2-only) implementation did.
fn append_new_section(base: String, heading: &str, content: &str) -> String {
    let mut result = base;
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(&format!("\n## {heading}\n\n{content}\n"));
    result
}

/// Per-line "is this line inside (or itself a delimiter of) a fenced code
/// block" mask, so heading detection never mistakes a `#`-prefixed line
/// inside a ` ``` ` or `~~~` fence for a real section heading.
pub(crate) fn fence_mask(lines: &[&str]) -> Vec<bool> {
    scan_fences(lines).0
}

fn scan_fences(lines: &[&str]) -> (Vec<bool>, bool) {
    let mut scanner = super::scanner::Scanner::default();
    let mask = lines.iter().map(|line| scanner.fenced(line)).collect();
    (mask, scanner.unclosed())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_h3_nested_under_h2_group_heading() {
        let base = "## Group heading\n\n### Individual finding\n\nstale text\n".to_string();
        let (result, outcome) = splice_section(base, "Individual finding", "fixed text").unwrap();
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(
            result,
            "## Group heading\n\n### Individual finding\n\nfixed text\n"
        );
    }

    #[test]
    fn h3_section_ends_at_next_shallower_h2() {
        let base = "### A\n\nold a\n\n## B\n\nkeep b\n".to_string();
        let (result, outcome) = splice_section(base, "A", "new a").unwrap();
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(result, "### A\n\nnew a\n\n## B\n\nkeep b\n");
    }

    #[test]
    fn h3_section_ends_at_sibling_h3() {
        let base = "### A\n\nold a\n\n### B\n\nkeep b\n".to_string();
        let (result, outcome) = splice_section(base, "A", "new a").unwrap();
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(result, "### A\n\nnew a\n\n### B\n\nkeep b\n");
    }

    #[test]
    fn h3_section_swallows_deeper_h4() {
        let base = "### A\n\n#### Nested\n\nold a\n\n### B\n\nkeep b\n".to_string();
        let (result, outcome) = splice_section(base, "A", "new a").unwrap();
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(result, "### A\n\nnew a\n\n### B\n\nkeep b\n");
    }

    #[test]
    fn fenced_code_block_heading_is_not_a_section() {
        let base = "# T\n\n```markdown\n## Real\n```\n\n## Real\n\nold\n".to_string();
        let (result, outcome) = splice_section(base, "Real", "new").unwrap();
        assert_eq!(outcome, SectionOutcome::Replaced { level: 2 });
        assert_eq!(
            result,
            "# T\n\n```markdown\n## Real\n```\n\n## Real\n\nnew\n"
        );
    }

    #[test]
    fn absent_heading_appends_and_reports_appended() {
        let base = "# T\n".to_string();
        let (result, outcome) = splice_section(base, "New", "body").unwrap();
        assert_eq!(outcome, SectionOutcome::Appended);
        assert_eq!(result, "# T\n\n## New\n\nbody\n");
    }

    #[test]
    fn h1_is_never_a_target_even_with_exact_text_match() {
        let base = "# Title\n\nbody\n".to_string();
        let (_, outcome) = splice_section(base, "Title", "new body").unwrap();
        assert_eq!(outcome, SectionOutcome::Appended);
    }

    #[test]
    fn unclosed_fence_rejects_edits() {
        let base = "# T\n\n```\n## Real\n```\n\n## Real\n\nold\n\n~~~\nunclosed\n";
        assert_eq!(
            splice_section(base.to_string(), "Real", "new"),
            Err(SectionEditError::UnclosedFence)
        );
        assert_eq!(
            delete_section(base, "Real"),
            Err(SectionEditError::UnclosedFence)
        );
        assert_eq!(
            set_section_marker(base, "Real", "marker"),
            Err(SectionEditError::UnclosedFence)
        );
    }
}
