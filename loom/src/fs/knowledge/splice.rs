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

/// What [`splice_section`] actually did, so callers can report accurately
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
/// <heading>` section at EOF when no heading matches. Shared by
/// [`super::dir::KnowledgeDir::replace_section`] and
/// [`super::dir::KnowledgeDir::replace_section_target`].
pub(crate) fn splice_section(
    base: String,
    heading: &str,
    content: &str,
) -> (String, SectionOutcome) {
    let lines: Vec<&str> = base.lines().collect();
    let in_fence = fence_mask(&lines);

    let mut target: Option<(usize, usize)> = None;
    for (i, line) in lines.iter().enumerate() {
        if in_fence[i] {
            continue;
        }
        if let Some(level) = heading_level_of(line, heading) {
            target = Some((i, level));
            break;
        }
    }

    match target {
        Some((start, level)) => {
            let end = section_end(&lines, &in_fence, start, level);
            let replaced = assemble_replacement(&base, &lines, start, end, level, heading, content);
            (replaced, SectionOutcome::Replaced { level })
        }
        None => (
            append_new_section(base, heading, content),
            SectionOutcome::Appended,
        ),
    }
}

/// If `line` (trailing whitespace ignored) is an ATX heading, returns its
/// level (1-6) and the heading text after the hashes and required
/// whitespace. A bare `#`/`##`/... line with no text yields an empty string.
/// `None` for a non-heading line, including `###Foo` (no space - not a valid
/// ATX heading) and a run of 7+ hashes (past the ATX heading limit).
fn atx_heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end();
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    match rest.as_bytes().first() {
        None => Some((hashes, "")),
        Some(b' ') | Some(b'\t') => Some((hashes, rest.trim_start_matches([' ', '\t']))),
        _ => None,
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
fn section_end(lines: &[&str], in_fence: &[bool], start: usize, level: usize) -> usize {
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
    level: usize,
    heading: &str,
    content: &str,
) -> String {
    let hashes = "#".repeat(level);
    let mut result = String::new();
    for line in &lines[..start] {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!("{hashes} {heading}\n\n{content}\n"));
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
fn fence_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut open: Option<(u8, usize)> = None;

    for (i, line) in lines.iter().enumerate() {
        match (open, fence_delimiter(line)) {
            (None, Some(delim)) => {
                mask[i] = true;
                open = Some(delim);
            }
            (Some((ch, len)), Some((close_ch, close_len)))
                if close_ch == ch && close_len >= len =>
            {
                mask[i] = true;
                open = None;
            }
            (Some(_), _) => mask[i] = true,
            (None, None) => {}
        }
    }

    // An unclosed fence at EOF: treating every line from the opener onward
    // as "inside a fence" would make `section_end` find no terminator and
    // report the entire rest of the file as the matched section's body -
    // silent knowledge loss, made worse by `splice_section` still reporting
    // `SectionOutcome::Replaced` as if the correction succeeded. Degrade to
    // fence-unaware instead (the behaviour before fence tracking existed): a
    // heading inside the unclosed "fence" text might now match when it
    // shouldn't, but the worst case is a wrong, still-bounded span - never a
    // truncate-to-EOF deletion.
    if open.is_some() {
        return vec![false; lines.len()];
    }

    mask
}

/// `(delimiter byte, run length)` if `line`, once its leading whitespace is
/// trimmed, opens or closes a fenced code block (3+ backticks or tildes).
fn fence_delimiter(line: &str) -> Option<(u8, usize)> {
    let trimmed = line.trim_start();
    let byte = *trimmed.as_bytes().first()?;
    if byte != b'`' && byte != b'~' {
        return None;
    }
    let len = trimmed.bytes().take_while(|&b| b == byte).count();
    (len >= 3).then_some((byte, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_h3_nested_under_h2_group_heading() {
        let base = "## Group heading\n\n### Individual finding\n\nstale text\n".to_string();
        let (result, outcome) = splice_section(base, "Individual finding", "fixed text");
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(
            result,
            "## Group heading\n\n### Individual finding\n\nfixed text\n"
        );
    }

    #[test]
    fn h3_section_ends_at_next_shallower_h2() {
        let base = "### A\n\nold a\n\n## B\n\nkeep b\n".to_string();
        let (result, outcome) = splice_section(base, "A", "new a");
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(result, "### A\n\nnew a\n\n## B\n\nkeep b\n");
    }

    #[test]
    fn h3_section_ends_at_sibling_h3() {
        let base = "### A\n\nold a\n\n### B\n\nkeep b\n".to_string();
        let (result, outcome) = splice_section(base, "A", "new a");
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(result, "### A\n\nnew a\n\n### B\n\nkeep b\n");
    }

    #[test]
    fn h3_section_swallows_deeper_h4() {
        let base = "### A\n\n#### Nested\n\nold a\n\n### B\n\nkeep b\n".to_string();
        let (result, outcome) = splice_section(base, "A", "new a");
        assert_eq!(outcome, SectionOutcome::Replaced { level: 3 });
        assert_eq!(result, "### A\n\nnew a\n\n### B\n\nkeep b\n");
    }

    #[test]
    fn fenced_code_block_heading_is_not_a_section() {
        let base = "# T\n\n```markdown\n## Real\n```\n\n## Real\n\nold\n".to_string();
        let (result, outcome) = splice_section(base, "Real", "new");
        assert_eq!(outcome, SectionOutcome::Replaced { level: 2 });
        assert_eq!(
            result,
            "# T\n\n```markdown\n## Real\n```\n\n## Real\n\nnew\n"
        );
    }

    #[test]
    fn absent_heading_appends_and_reports_appended() {
        let base = "# T\n".to_string();
        let (result, outcome) = splice_section(base, "New", "body");
        assert_eq!(outcome, SectionOutcome::Appended);
        assert_eq!(result, "# T\n\n## New\n\nbody\n");
    }

    #[test]
    fn h1_is_never_a_target_even_with_exact_text_match() {
        let base = "# Title\n\nbody\n".to_string();
        let (_, outcome) = splice_section(base, "Title", "new body");
        assert_eq!(outcome, SectionOutcome::Appended);
    }

    #[test]
    fn unclosed_fence_degrades_to_fence_unaware_instead_of_deleting_to_eof() {
        // No closing ``` - a naive fence tracker would mask every line from
        // the opener to EOF, find no terminator for "## Real", and replace
        // the entire tail of the file (reporting `Replaced` while doing it).
        let base = "# T\n\n```\nsome code\n\n## Real\n\nold\n".to_string();
        let (result, outcome) = splice_section(base, "Real", "new");
        assert_eq!(outcome, SectionOutcome::Replaced { level: 2 });
        assert_eq!(result, "# T\n\n```\nsome code\n\n## Real\n\nnew\n");
    }
}
